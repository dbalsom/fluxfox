/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024 Daniel Balsom

    Permission is hereby granted, free of charge, to any person obtaining a
    copy of this software and associated documentation files (the “Software”),
    to deal in the Software without restriction, including without limitation
    the rights to use, copy, modify, merge, publish, distribute, sublicense,
    and/or sell copies of the Software, and to permit persons to whom the
    Software is furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in
    all copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
    FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
    DEALINGS IN THE SOFTWARE.

    --------------------------------------------------------------------------

    src/copy_protection.rs

    Contains an enum for various copy protection schemes and code to detect
    such.
*/

use crate::{types::chs::DiskChsnQuery, DiskImage};
use std::fmt::{Display, Formatter, Result};

#[derive(Copy, Clone, Debug)]
pub enum CopyProtectionScheme {
    FormasterCopyLock(u8),
    SoftguardSuperlok(u8),
    EaInterlock(u8),
    VaultProlok,
    XemagXelok(u8),
    HlsDuplication,
    Undetermined,
}

impl Display for CopyProtectionScheme {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            CopyProtectionScheme::FormasterCopyLock(v) => write!(f, "Formaster CopyLock v{}", v),
            CopyProtectionScheme::SoftguardSuperlok(_v) => write!(f, "Softguard Superlok"),
            CopyProtectionScheme::EaInterlock(v) => write!(f, "EA Interlock v{}", v),
            CopyProtectionScheme::VaultProlok => write!(f, "Vault Prolok"),
            CopyProtectionScheme::XemagXelok(v) => write!(f, "XEMAG Xelok v{}", v),
            CopyProtectionScheme::HlsDuplication => write!(f, "HLS Duplication"),
            CopyProtectionScheme::Undetermined => write!(f, "Likely protected, but scheme undetermined"),
        }
    }
}

impl DiskImage {
    /// Attempt to determine the copy protection scheme used on the disk image.
    /// Returns None if no copy protection is detected.
    pub fn detect_copy_protection(&self) -> Option<CopyProtectionScheme> {
        for track in self.track_iter() {
            let track_ch = track.ch();

            // Check for Formaster CopyLock.
            // Look for Sector 1 on a track with n == 1 and bad crc.
            // If the address crc is also bad, it's version 2.
            if let Ok(scan_result) =
                track.scan_sector(DiskChsnQuery::new(track_ch.c(), track_ch.h(), 1, 1), Some(1), None)
            {
                if scan_result.data_crc_error {
                    return if scan_result.address_crc_error {
                        Some(CopyProtectionScheme::FormasterCopyLock(2))
                    }
                    else {
                        Some(CopyProtectionScheme::FormasterCopyLock(1))
                    };
                }
            }

            // Check for Softguard Superlok.
            // Look for Sector 1 on a track > 1 with n == 6 (8129 bytes) and bad crc.
            // Not sure how to detect v2 as the main change is in the detection code.
            if track_ch.c() > 1 {
                if let Ok(scan_result) =
                    track.scan_sector(DiskChsnQuery::new(track_ch.c(), track_ch.h(), 1, 6), Some(6), None)
                {
                    if scan_result.data_crc_error {
                        return Some(CopyProtectionScheme::SoftguardSuperlok(1));
                    }
                }
            }

            // Check for EA Interlock.
            // If a track has 96 sectors, that's a clear indication.
            if track.sector_ct() == 96 {
                return Some(CopyProtectionScheme::EaInterlock(1));
            }
        }

        None
    }
}
