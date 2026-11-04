// ==== Generic Company Inc. portfolio ====================================
// Project: ticket_tracker | File: tests/icon.rs
// About: Tests that the pixel icon files agree with each other and keep their credit.
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's tools/stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! Tests that the pixel icon files agree with each other and keep their credit.
//!
//! The pixel icon: app.ico holds 16/32/48 px as BMP and 256 px as PNG, the BMP sizes are the 16 px image
//! enlarged by whole pixels, and app-32.rgba (the window icon) is the 32 px image.
//!
//! The icon files are made by the portfolio's tools/pixel_icons.py, not by this crate; these tests guard that the
//! committed files stay consistent with each other and with what src/main.rs and the installer expect.
//!
//! How an .ico file is laid out (the Windows ICONDIR format):
//! - A 6-byte header: reserved (0), type (1 = icon), then the image count as a little-endian u16 at
//!   offset 4.
//! - One 16-byte directory entry per image: width and height as single bytes (0 means 256), colour
//!   count, reserved, planes and bit count, then the image's byte size (u32 at +8) and its offset in
//!   the file (u32 at +12).
//! - The images. Each is either a whole PNG file, or a BMP without its 14-byte file header: a 40-byte
//!   BITMAPINFOHEADER whose height is doubled (the colour rows plus a 1-bit AND mask below them), then
//!   32-bit BGRA pixel rows stored bottom row first.
//!
//! app-32.rgba is simpler: width and height as little-endian u32s, then RGBA pixels top row first.

/// The icon file the installer writes for shortcuts (src/main.rs `icon_ico`).
const ICO: &[u8] = include_bytes!("../icon/app.ico");
/// The raw 32 px window icon (src/main.rs `icon_rgba`).
const RGBA: &[u8] = include_bytes!("../icon/app-32.rgba");

/// The little-endian u16 at byte `i` of `b`, widened for indexing.
fn u16le(b: &[u8], i: usize) -> usize {
    u16::from_le_bytes([b[i], b[i + 1]]) as usize
}
/// The little-endian u32 at byte `i` of `b`, widened for indexing.
fn u32le(b: &[u8], i: usize) -> usize {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize
}

/// (size, image bytes) for every entry.
///
/// Reads the image count from the header, then each 16-byte directory entry after it; the slice is
/// the entry's bytes at its offset (+12) for its length (+8).
fn entries() -> Vec<(usize, &'static [u8])> {
    (0..u16le(ICO, 4))
        .map(|i| {
            // directory entries start after the 6-byte header
            let e = 6 + 16 * i;
            // a width byte of 0 stands for 256, which does not fit in a byte
            let size = if ICO[e] == 0 { 256 } else { ICO[e] as usize };
            (size, &ICO[u32le(ICO, e + 12)..u32le(ICO, e + 12) + u32le(ICO, e + 8)])
        })
        .collect()
}

/// RGBA at (x, y) from a 32-bit BMP icon entry (BGRA rows, bottom-up).
///
/// `y` counts from the top as in the picture; the row is flipped because BMP stores the bottom row
/// first. Pixels start right after the 40-byte header (32-bit images have no palette).
fn bmp_pixel(entry: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
    let i = 40 + ((size - 1 - y) * size + x) * 4;
    [entry[i + 2], entry[i + 1], entry[i], entry[i + 3]]
}

/// Pins the four sizes in order, PNG for 256 px (by its 8-byte signature) and a BMP header for the
/// rest: header size 40, width = size, height = 2 * size for the colour rows plus the AND mask.
#[test]
fn sizes_and_formats() {
    let e = entries();
    assert_eq!(e.iter().map(|(s, _)| *s).collect::<Vec<_>>(), vec![16, 32, 48, 256]);
    assert_eq!(&e[3].1[..8], b"\x89PNG\r\n\x1a\n");
    for (size, blob) in &e[..3] {
        assert_eq!(u32le(blob, 0), 40);
        assert_eq!(u32le(blob, 4), *size);
        assert_eq!(u32le(blob, 8), size * 2);
    }
}

/// Pins nearest-neighbour enlargement: every 32 and 48 px pixel equals the 16 px pixel its 2x2 or 3x3
/// block came from, and app-32.rgba holds exactly the 32 px BMP's pixels after its 8-byte size header.
#[test]
fn whole_pixel_blocks() {
    let e = entries();
    let (small, double, triple) = (e[0].1, e[1].1, e[2].1);
    for y in 0..48 {
        for x in 0..48 {
            assert_eq!(bmp_pixel(triple, 48, x, y), bmp_pixel(small, 16, x / 3, y / 3));
            if x < 32 && y < 32 {
                assert_eq!(bmp_pixel(double, 32, x, y), bmp_pixel(small, 16, x / 2, y / 2));
                // skip the 8-byte width/height header; rows run top-down here, unlike the BMP
                let i = 8 + (y * 32 + x) * 4;
                assert_eq!(&RGBA[i..i + 4], &bmp_pixel(double, 32, x, y));
            }
        }
    }
    assert_eq!((u32le(RGBA, 0), u32le(RGBA, 4), RGBA.len()), (32, 32, 8 + 32 * 32 * 4));
}

/// Pins that the icon's README keeps the CC BY 2.5 credit to its author, which the licence requires.
#[test]
fn credited() {
    let readme = include_str!("../icon/README.md");
    assert!(readme.contains("Creative Commons Attribution 2.5") && readme.contains("Mark James"));
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=3ffa7ae69ed713e544d7745ec5ac4a20e2237e4aa40dc382170aaca14aaa4418 ====
