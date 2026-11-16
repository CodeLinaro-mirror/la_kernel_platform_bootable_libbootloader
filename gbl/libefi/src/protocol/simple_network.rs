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

//! Rust wrapper for `EFI_SIMPLE_NETWORK_PROTOCOL`.

use crate::efi_call;
use crate::{
    protocol::{Protocol, ProtocolInfo, Requirement},
    versioned_protocol,
};
use core::{ffi::c_void, ptr::NonNull};
use efi_types::{
    EfiGuid, EfiMacAddress, EfiSimpleNetworkMode, EfiSimpleNetworkProtocol,
    EFI_SIMPLE_NETWORK_PROTOCOL_GUID_U64_0, EFI_SIMPLE_NETWORK_PROTOCOL_GUID_U64_1,
    EFI_SIMPLE_NETWORK_PROTOCOL_REVISION, EFI_SIMPLE_NETWORK_RECEIVE_PROMISCUOUS,
    EFI_SIMPLE_NETWORK_RECEIVE_PROMISCUOUS_MULTICAST, EFI_SIMPLE_NETWORK_RECEIVE_UNICAST,
};
use liberror::{Error, Result};

/// EFI_SIMPLE_NETWORK_PROTOCOL
pub struct SimpleNetworkProtocol;

versioned_protocol!(SimpleNetworkProtocol, EFI_SIMPLE_NETWORK_PROTOCOL_REVISION);

impl ProtocolInfo for SimpleNetworkProtocol {
    type InterfaceType = EfiSimpleNetworkProtocol;

    const GUID: EfiGuid = EfiGuid::from_u64s(
        EFI_SIMPLE_NETWORK_PROTOCOL_GUID_U64_0,
        EFI_SIMPLE_NETWORK_PROTOCOL_GUID_U64_1,
    );

    const REQUIREMENT: Requirement = Requirement::Optional;
}

impl<'a> Protocol<'a, SimpleNetworkProtocol> {
    /// Wrapper of `EFI_SIMPLE_NETWORK.Start()`
    pub fn start(&self) -> Result<()> {
        // SAFETY:
        // `self.interface_ptr()` guarantees to return a valid object pointer as established by
        // `Protocol::new()`.
        // `self.interface_ptr()` outlives the call and will not be retained.
        unsafe { efi_call!(self.interface().start, self.interface_ptr()) }
    }

    /// Wrapper of `EFI_SIMPLE_NETWORK.Stop()`
    pub fn stop(&self) -> Result<()> {
        // SAFETY: See safety reasoning of `start()`.
        unsafe { efi_call!(self.interface().stop, self.interface_ptr()) }
    }

    /// Wrapper of `EFI_SIMPLE_NETWORK.Initialize()`
    pub fn initialize(&self, extra_rx_buf_size: usize, extra_tx_buf_size: usize) -> Result<()> {
        // SAFETY: See safety reasoning of `start()`.
        unsafe {
            efi_call!(
                self.interface().initialize,
                self.interface_ptr(),
                extra_rx_buf_size,
                extra_tx_buf_size
            )
        }
    }

    /// Wrapper of `EFI_SIMPLE_NETWORK.Reset()`
    pub fn reset(&self, extended_verification: bool) -> Result<()> {
        // SAFETY: See safety reasoning of `start()`.
        unsafe { efi_call!(self.interface().reset, self.interface_ptr(), extended_verification) }
    }

    /// Wrapper of `EFI_SIMPLE_NETWORK.Shutdown()`
    pub fn shutdown(&self) -> Result<()> {
        // SAFETY: See safety reasoning of `start()`.
        unsafe { efi_call!(self.interface().shutdown, self.interface_ptr()) }
    }

    /// Wrapper of `EFI_SIMPLE_NETWORK.ReceiveFilters()`
    pub fn receive_filters(
        &self,
        enable: u32,
        disable: u32,
        reset_mcast_filter: bool,
        mcast_filter: &mut [EfiMacAddress],
    ) -> Result<()> {
        // SAFETY:
        // `self.interface_ptr()` guarantees to return a valid object pointer as established by
        // `Protocol::new()`.
        // `self.interface_ptr()` outlives the call and will not be retained.
        // `mcast_filter` is for input only. It outlives the call and will not be retained.
        unsafe {
            efi_call!(
                self.interface().receive_filters,
                self.interface_ptr(),
                enable,
                disable,
                reset_mcast_filter,
                mcast_filter.len(),
                mcast_filter.as_mut_ptr()
            )
        }
    }

    /// Wrapper of `EFI_SIMPLE_NETWORK.GetStatus()`
    pub fn get_status(
        &self,
        interrupt_status: Option<&mut u32>,
        recycle_buffer: Option<&mut *mut c_void>,
    ) -> Result<()> {
        // SAFETY:
        // See safety reasoning of `start()`.
        // Pointers to `interrupt_status`, `recycled_buffer` are valid during the call and for
        // writing output values only.
        unsafe {
            efi_call!(
                self.interface().get_status,
                self.interface_ptr(),
                option_ref_mut_to_pointer(interrupt_status),
                option_ref_mut_to_pointer(recycle_buffer)
            )?;
        }
        Ok(())
    }

    /// Wrapper of `EFI_SIMPLE_NETWORK.Transmit()`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// * `buf` points to a single live allocation of `buf.len()` initialized bytes valid for reads
    ///   and writes.
    /// * No Rust references (`&` or `&mut`) or concurrent accesses alias the memory pointed to by
    ///   `buf` during the call.
    /// * If this method returns `Ok(())`, the network interface retains `buf` asynchronously: the
    ///   memory pointed to by `buf` must remain live and must not be read, written, or referenced
    ///   by Rust until either 1) its address is returned in `recycle_buffer` by
    ///   [`Self::get_status`] or 2) [`Self::shutdown`] returns `Ok(())` or
    ///   `Err(Error::NotStarted)`. (If this method returns `Err(_)`, `buf` is not retained.)
    pub unsafe fn transmit(
        &self,
        header_size: usize,
        buf: NonNull<[u8]>,
        mut src: EfiMacAddress,
        mut dest: EfiMacAddress,
        mut protocol: u16,
    ) -> Result<()> {
        // SAFETY:
        // * `self.interface_ptr()` points to a valid `EfiSimpleNetworkProtocol` instance as
        //   established by `Protocol::new()`, outlives the call, and is not retained.
        // * `&mut src`, `&mut dest`, and `&mut protocol` point to local stack variables that are
        //   valid for the duration of the call and are not retained by the network interface.
        // * `buf` is non-null by type, and by the caller's `# Safety` contract points to
        //   `buf.len()` live, initialized, unaliased bytes that—if `transmit` succeeds—remain
        //   valid and unaccessed until recycled by `get_status()` or released by `shutdown()`.
        unsafe {
            efi_call!(
                self.interface().transmit,
                self.interface_ptr(),
                header_size,
                buf.len(),
                buf.cast::<c_void>().as_ptr(),
                &mut src,
                &mut dest,
                &mut protocol
            )
        }
    }

    /// Wrapper of `EFI_SIMPLE_NETWORK.Receive()`.
    pub fn receive(
        &self,
        header_size: Option<&mut usize>,
        buf_size: Option<&mut usize>,
        buf: &mut [u8],
        src: Option<&mut EfiMacAddress>,
        dest: Option<&mut EfiMacAddress>,
        protocol: Option<&mut u16>,
    ) -> Result<()> {
        // SAFETY:
        // See safety reasoning of `start()`.
        // All pointers passed are valid, outlive the call and are not retained by the call.
        unsafe {
            efi_call!(
                self.interface().receive,
                self.interface_ptr(),
                option_ref_mut_to_pointer(header_size),
                option_ref_mut_to_pointer(buf_size),
                buf.as_mut_ptr() as *mut _,
                option_ref_mut_to_pointer(src),
                option_ref_mut_to_pointer(dest),
                option_ref_mut_to_pointer(protocol)
            )?;
        }
        Ok(())
    }

    /// Returns `EFI_SIMPLE_NETWORK.Mode` structure
    pub fn mode(&self) -> Result<EfiSimpleNetworkMode> {
        // SAFETY: Non-null pointer from UEFI interface points to valid object.
        unsafe { self.interface().mode.as_ref() }.ok_or(Error::NotFound).copied()
    }

    /// Sets to promiscuous mode to receive all packets over the network.
    pub fn set_promiscuous_mode(&self) -> Result<()> {
        self.receive_filters(
            EFI_SIMPLE_NETWORK_RECEIVE_UNICAST
                | EFI_SIMPLE_NETWORK_RECEIVE_PROMISCUOUS
                | EFI_SIMPLE_NETWORK_RECEIVE_PROMISCUOUS_MULTICAST,
            0,
            false,
            &mut [],
        )
    }
}

/// A helper to convert an `Option<&mut T>` to `*mut T`. None maps to NULL.
fn option_ref_mut_to_pointer<T>(option: Option<&mut T>) -> *mut T {
    option.map(|t| t as *mut _).unwrap_or_default()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::run_test_with_mock_protocol;
    use efi_types::{EfiStatus, EFI_STATUS_NOT_READY, EFI_STATUS_SUCCESS};

    #[test]
    fn test_transmit_forwards_raw_buffer_and_headers() {
        const HEADER_SIZE: usize = 14;
        const PAYLOAD: [u8; 6] = [0xde, 0xad, 0xbe, 0xef, 0x01, 0x02];
        const PROTOCOL: u16 = 0x86dd;

        unsafe extern "efiapi" fn c_transmit(
            _: *mut EfiSimpleNetworkProtocol,
            header_size: usize,
            buf_size: usize,
            buf: *mut c_void,
            src: *mut EfiMacAddress,
            dest: *mut EfiMacAddress,
            protocol: *mut u16,
        ) -> EfiStatus {
            assert_eq!(header_size, HEADER_SIZE);
            assert_eq!(buf_size, PAYLOAD.len());
            assert!(!buf.is_null(), "expected non-null transmit buffer");
            // SAFETY: `buf`, `src`, `dest`, and `protocol` are valid for the duration of this
            // mock callback as passed by `SimpleNetworkProtocol::transmit`.
            unsafe {
                assert_eq!(core::slice::from_raw_parts(buf.cast::<u8>(), buf_size), &PAYLOAD);
                assert_eq!((*src).addr[0], 0x11);
                assert_eq!((*dest).addr[0], 0x22);
                assert_eq!(*protocol, PROTOCOL);
            }
            EFI_STATUS_SUCCESS
        }

        let mut src = EfiMacAddress::default();
        src.addr[0] = 0x11;
        let mut dest = EfiMacAddress::default();
        dest.addr[0] = 0x22;
        let mut frame = PAYLOAD;
        let raw_buf = NonNull::from(&mut frame[..]);

        let c_interface =
            EfiSimpleNetworkProtocol { transmit: Some(c_transmit), ..Default::default() };
        run_test_with_mock_protocol::<SimpleNetworkProtocol>(c_interface, |snp| {
            // SAFETY: `raw_buf` points to `frame`, which has no active references and outlives the
            // mock callback (which does not retain `raw_buf`).
            assert_eq!(unsafe { snp.transmit(HEADER_SIZE, raw_buf, src, dest, PROTOCOL) }, Ok(()));
        });
    }

    #[test]
    fn test_transmit_propagates_not_ready() {
        unsafe extern "efiapi" fn c_not_ready(
            _: *mut EfiSimpleNetworkProtocol,
            _: usize,
            _: usize,
            _: *mut c_void,
            _: *mut EfiMacAddress,
            _: *mut EfiMacAddress,
            _: *mut u16,
        ) -> EfiStatus {
            EFI_STATUS_NOT_READY
        }

        let mut frame = [0u8; 4];
        let raw_buf = NonNull::from(&mut frame[..]);
        let c_interface =
            EfiSimpleNetworkProtocol { transmit: Some(c_not_ready), ..Default::default() };
        run_test_with_mock_protocol::<SimpleNetworkProtocol>(c_interface, |snp| {
            // SAFETY: `raw_buf` points to `frame`, has no active references, and is not retained
            // when `transmit` returns `Err(Error::NotReady)`.
            let res = unsafe {
                snp.transmit(0, raw_buf, EfiMacAddress::default(), EfiMacAddress::default(), 0)
            };
            assert_eq!(res, Err(Error::NotReady));
        });
    }
}
