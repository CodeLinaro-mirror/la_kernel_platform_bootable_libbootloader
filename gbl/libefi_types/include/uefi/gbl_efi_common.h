/*
 * Copyright (C) 2024 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 * SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause-Patent
 *
 * You may choose to use or redistribute this file under
 *  (a) the Apache License, Version 2.0, or
 *  (b) the BSD 2-Clause Patent license.
 *
 * Unless you expressly elect the BSD-2-Clause-Patent terms, the Apache-2.0
 * terms apply by default.
 */

#ifndef __GBL_EFI_COMMON_H__
#define __GBL_EFI_COMMON_H__

#include "gbl_protocol_utils.h"
#include "types.h"

EFI_GUID(GBL_EFI_VENDOR_GUID, 0x5a6d92f3, 0xa2d0, 0x4083, 0x91, 0xa1, 0xa5,
         0x0f, 0x6c, 0x3d, 0x98, 0x30);

#define GBL_EFI_OS_BOOT_TARGET_VARNAME "gbl_os_boot_fuchsia"

// `EFI_RUNTIME_SERVICES.ResetSystem()` `ResetData` passed by GBL for non-normal
// reboots: a NUL-terminated CHAR16 reason string, followed by
// `GBL_EFI_RESET_DATA_GUID` and a UINT8 bitmask of `GBL_EFI_RESET_DATA_FLAG_*`.
// See gbl/docs/efi_integration.md.
#define GBL_EFI_RESET_DATA_BOOTLOADER "bootloader"
#define GBL_EFI_RESET_DATA_FASTBOOT "fastboot"
#define GBL_EFI_RESET_DATA_RECOVERY "recovery"

EFI_GUID(GBL_EFI_RESET_DATA_GUID, 0xb86bef58, 0x5d90, 0x41f1, 0xb7, 0x21, 0xb3,
         0xe3, 0xb3, 0x15, 0x05, 0xfc);

// GBL failed to persist the reboot target (e.g. missing `misc`). Firmware
// may report it once via
// `GBL_EFI_BOOT_CONTROL_PROTOCOL.GetOneShotBootMode()` on the next boot.
#define GBL_EFI_RESET_DATA_FLAG_ONE_SHOT 0x01

#define PARTITION_NAME_LEN_U16 36

#endif /* __GBL_EFI_COMMON_H__ */
