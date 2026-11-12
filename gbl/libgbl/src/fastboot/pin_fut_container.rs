// Copyright 2024, The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use core::{future::Future, mem::replace, num::NonZeroU64, pin::Pin};
use gbl_async::poll_with_local_trace_config;
use libgblutils::shared::Shared;

/// Per-session scheduling state shared between the foreground fastboot command loop and background
/// async tasks.
pub(crate) struct AsyncTaskState {
    is_head_task: bool,
    waiting_for_tasks: usize,
    sync_write_budget: bool,
    max_sync_write_slice: Option<NonZeroU64>,
}

impl Default for AsyncTaskState {
    fn default() -> Self {
        Self {
            is_head_task: true,
            waiting_for_tasks: 0,
            sync_write_budget: true,
            max_sync_write_slice: None,
        }
    }
}

impl AsyncTaskState {
    /// Returns whether the currently polled async task is at the front of the task queue (or
    /// running synchronously outside `poll_all`).
    pub(crate) fn is_head_task(&self) -> bool {
        self.is_head_task
    }

    /// Returns the active synchronous write slice limit when the active transport specifies one and
    /// the foreground loop is not blocked waiting for background tasks to finish.
    pub(crate) fn effective_sync_write_slice(&self) -> Option<NonZeroU64> {
        self.max_sync_write_slice.filter(|_| self.waiting_for_tasks == 0)
    }

    /// Claims the single synchronous write slice budget for the current `poll_all` pass.
    ///
    /// Replenished at the start of each `poll_all` invocation (`gbl_fb_tasks` and the single
    /// post-enqueue poll in `schedule_task` after a command's download has already been drained).
    pub(crate) fn take_sync_write_budget(&mut self) -> bool {
        replace(&mut self.sync_write_budget, false)
    }

    #[cfg(test)]
    pub(crate) fn is_waiting_for_tasks(&self) -> bool {
        self.waiting_for_tasks > 0
    }
}

/// RAII guard marking that the foreground fastboot session is blocked waiting on background async
/// tasks, allowing synchronous storage backends to drain remaining writes without slicing.
#[must_use]
pub(crate) struct WaitingForAsyncTasksGuard<'a>(Option<&'a Shared<AsyncTaskState>>);

impl<'a> WaitingForAsyncTasksGuard<'a> {
    /// Enters a foreground wait on background async tasks when `state` is `Some`.
    pub(crate) fn new(state: Option<&'a Shared<AsyncTaskState>>) -> Self {
        if let Some(s) = state {
            s.borrow_mut().waiting_for_tasks += 1;
        }
        Self(state)
    }
}

impl Drop for WaitingForAsyncTasksGuard<'_> {
    fn drop(&mut self) {
        if let Some(s) = self.0 {
            s.borrow_mut().waiting_for_tasks -= 1;
        }
    }
}

/// RAII guard configuring the active transport's synchronous write slice limit on
/// [`AsyncTaskState`] while commands or sessions on that transport are processed, clearing the
/// limit on drop (guards are not nested).
#[must_use]
pub(crate) struct ActiveTransportGuard<'a>(Option<&'a Shared<AsyncTaskState>>);

impl<'a> ActiveTransportGuard<'a> {
    /// Sets `max_sync_write_slice` for the lifetime of the guard when `state` is `Some`.
    pub(crate) fn new(
        state: Option<&'a Shared<AsyncTaskState>>,
        max_sync_write_slice: Option<NonZeroU64>,
    ) -> Self {
        if let Some(s) = state {
            s.borrow_mut().max_sync_write_slice = max_sync_write_slice;
        }
        Self(state)
    }
}

impl Drop for ActiveTransportGuard<'_> {
    fn drop(&mut self) {
        if let Some(s) = self.0 {
            s.borrow_mut().max_sync_write_slice = None;
        }
    }
}

#[must_use]
struct PollPassGuard<'a> {
    state: Option<&'a Shared<AsyncTaskState>>,
    prev_is_head_task: bool,
}

impl<'a> PollPassGuard<'a> {
    fn new(state: Option<&'a Shared<AsyncTaskState>>) -> Self {
        let prev_is_head_task = if let Some(s) = state {
            let mut s = s.borrow_mut();
            let prev = s.is_head_task;
            s.sync_write_budget = true;
            prev
        } else {
            true
        };
        Self { state, prev_is_head_task }
    }

    fn set_head_task(&self, is_head: bool) {
        if let Some(s) = self.state {
            s.borrow_mut().is_head_task = is_head;
        }
    }
}

impl Drop for PollPassGuard<'_> {
    fn drop(&mut self) {
        if let Some(s) = self.state {
            s.borrow_mut().is_head_task = self.prev_is_head_task;
        }
    }
}

/// Represents a future local context data.
#[derive(Default)]
pub struct FutContext {
    /// Async local trace config.
    pub trace_config: bool,
}

/// A container abstraction that takes input of dynamically typed [Future]s and stores them at
/// pinned memory locations.
pub trait PinFutContainer<'a> {
    /// Adds and pins a new [Future] of any type generated by the given closure `f`.
    ///
    /// If operation cannot be performed, such as due to no capacity, `f` should not be called.
    fn add_with<F: Future<Output = ()> + 'a>(&mut self, f: impl FnOnce() -> F);

    /// Calls the closure on each element in the container in FIFO insertion order, removing any
    /// element for which it returns `true`.
    fn for_each_remove_if(
        &mut self,
        cb: impl FnMut(&mut Pin<&mut (dyn Future<Output = ()> + 'a)>, &mut FutContext) -> bool,
    );
}

/// An internal container abstraction that takes input of a specific type of [Future] and stores
/// them at pinned memory locations.
pub(crate) trait PinFutContainerTyped<'a, F: Future + 'a> {
    /// Adds and pins a new [Future] of type T into the container returned by `f`.
    ///
    /// If operation cannot be performed, such as due to no capacity, `f` should not be called.
    fn add_with(&mut self, f: impl FnOnce() -> F);

    /// Calls the closure on each element in the container in FIFO insertion order, removing any
    /// element for which it returns `true`.
    fn for_each_remove_if(
        &mut self,
        cb: impl FnMut(&mut Pin<&mut (dyn Future<Output = ()> + 'a)>, &mut FutContext) -> bool,
    );

    /// Polls all the [Future] once. Returns the number or unfinished ones.
    fn poll_all(&mut self, state: Option<&Shared<AsyncTaskState>>) -> usize {
        let pass_guard = PollPassGuard::new(state);
        let mut res = 0;
        self.for_each_remove_if(|v, b| {
            pass_guard.set_head_task(res == 0);
            let finished = poll_with_local_trace_config(v, &mut b.trace_config).is_some();
            res += usize::from(!finished);
            finished
        });
        res
    }

    /// Runs until all futures are finished
    #[cfg(test)]
    fn run(&mut self) {
        while self.poll_all(None) > 0 {}
    }

    /// Runs until all futures are finished, updating `state` on each poll pass.
    #[cfg(test)]
    fn run_with_state(&mut self, state: &Shared<AsyncTaskState>) {
        while self.poll_all(Some(state)) > 0 {}
    }
}

/// `PinFutContainer` can implement `PinFutContainerTyped` for any [Future] type.
impl<'a, F: Future<Output = ()> + 'a, T: PinFutContainer<'a>> PinFutContainerTyped<'a, F> for T {
    fn add_with(&mut self, f: impl FnOnce() -> F) {
        PinFutContainer::add_with(self, f)
    }

    fn for_each_remove_if(
        &mut self,
        cb: impl FnMut(&mut Pin<&mut (dyn Future<Output = ()> + 'a)>, &mut FutContext) -> bool,
    ) {
        PinFutContainer::for_each_remove_if(self, cb)
    }
}

/// An implementation of `PinFutContainerTyped` backed by a preallocated slice.
pub(crate) struct PinFutSlice<'a, F> {
    arr: &'a mut [(Pin<&'a mut F>, FutContext)],
    used: usize,
}

impl<'a, F> PinFutSlice<'a, F> {
    /// Creates a new instance
    pub fn new(arr: &'a mut [(Pin<&'a mut F>, FutContext)]) -> Self {
        Self { arr, used: 0 }
    }
}

impl<'a, F: Future<Output = ()> + 'a> PinFutContainerTyped<'a, F> for PinFutSlice<'a, F> {
    fn add_with(&mut self, f: impl FnOnce() -> F) {
        if self.used < self.arr.len() {
            self.arr[self.used].0.set(f());
            self.used += 1;
        }
    }

    fn for_each_remove_if(
        &mut self,
        mut cb: impl FnMut(&mut Pin<&mut (dyn Future<Output = ()> + 'a)>, &mut FutContext) -> bool,
    ) {
        // Polls in FIFO order; rotating `Pin<&mut F>` pointers preserves the pinned `F` storage.
        let mut idx = 0;
        while idx < self.used {
            if cb(&mut (self.arr[idx].0.as_mut() as _), &mut self.arr[idx].1) {
                self.arr[idx..self.used].rotate_left(1);
                self.used -= 1;
            } else {
                idx += 1;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use gbl_async::yield_now;

    #[test]
    fn test_async_task_state_guards_and_head_tracking() {
        let state = Shared::from(AsyncTaskState::default());
        assert!(state.borrow().is_head_task());
        assert!(!state.borrow().is_waiting_for_tasks());

        // `None` guard is a safe no-op; nested `Some` guards reference-count foreground waits.
        {
            let _noop = WaitingForAsyncTasksGuard::new(None);
            assert!(!state.borrow().is_waiting_for_tasks());
            let _g1 = WaitingForAsyncTasksGuard::new(Some(&state));
            assert!(state.borrow().is_waiting_for_tasks());
            {
                let _g2 = WaitingForAsyncTasksGuard::new(Some(&state));
                assert!(state.borrow().is_waiting_for_tasks());
            }
            assert!(state.borrow().is_waiting_for_tasks());
        }
        assert!(!state.borrow().is_waiting_for_tasks());

        // FIFO `poll_all` marks only the oldest unfinished task as the head task.
        let log = Shared::from(Vec::new());
        let mut tasks: Vec<(Pin<Box<dyn Future<Output = ()>>>, FutContext)> = Vec::new();
        PinFutContainer::add_with(&mut tasks, || {
            let (state, log) = (&state, &log);
            async move {
                log.borrow_mut().push((0, state.borrow().is_head_task()));
                yield_now().await;
                log.borrow_mut().push((0, state.borrow().is_head_task()));
            }
        });
        PinFutContainer::add_with(&mut tasks, || {
            let (state, log) = (&state, &log);
            async move {
                log.borrow_mut().push((1, state.borrow().is_head_task()));
                yield_now().await;
                log.borrow_mut().push((1, state.borrow().is_head_task()));
                yield_now().await;
                log.borrow_mut().push((1, state.borrow().is_head_task()));
            }
        });

        assert_eq!(
            PinFutContainerTyped::<core::future::Pending<()>>::poll_all(&mut tasks, Some(&state)),
            2
        );
        // Pass 2 completes task 0 (`res` stays 0), so task 1 immediately becomes the head task in
        // pass 2 and finishes in pass 3.
        assert_eq!(
            PinFutContainerTyped::<core::future::Pending<()>>::poll_all(&mut tasks, Some(&state)),
            1
        );
        assert_eq!(
            PinFutContainerTyped::<core::future::Pending<()>>::poll_all(&mut tasks, Some(&state)),
            0
        );
        assert_eq!(*log.borrow(), vec![(0, true), (1, false), (0, true), (1, true), (1, true)]);
        assert!(state.borrow().is_head_task());

        // `ActiveTransportGuard` configures `effective_sync_write_slice` (suppressed during
        // foreground waits) and clears the slice limit on drop.
        let slice_4k = NonZeroU64::new(4096);
        assert_eq!(state.borrow().effective_sync_write_slice(), None);
        {
            let _guard = ActiveTransportGuard::new(Some(&state), slice_4k);
            assert_eq!(state.borrow().effective_sync_write_slice(), slice_4k);
            {
                let _wait = WaitingForAsyncTasksGuard::new(Some(&state));
                assert_eq!(state.borrow().effective_sync_write_slice(), None);
            }
            assert_eq!(state.borrow().effective_sync_write_slice(), slice_4k);
        }
        assert_eq!(state.borrow().effective_sync_write_slice(), None);
    }
}
