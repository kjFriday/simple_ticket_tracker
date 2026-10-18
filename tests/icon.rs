// ==== ACME Inc. portfolio ===============================================
// Project: simple_ticket_tracker | File: tests/icon.rs
// About: The pixel icon: app.ico holds 16/32/48 px as BMP and 256 px as PNG, the BMP sizes are the...
// Copyright (c) 2026 kjFriday | SPDX-License-Identifier: AGPL-3.0-only
// You may change this file under the AGPL-3.0. The stamp at the end marks
// the version kjFriday wrote. Verify with the portfolio's stamp.py --check.
// Reused code, if any, is mapped in PROVENANCE.md.
// ========================================================================

//! The pixel icon: app.ico holds 16/32/48 px as BMP and 256 px as PNG, the BMP sizes are the 16 px image
//! enlarged by whole pixels, and app-32.rgba (the window icon) is the 32 px image.

const ICO: &[u8] = include_bytes!("../icon/app.ico");
const RGBA: &[u8] = include_bytes!("../icon/app-32.rgba");

fn u16le(b: &[u8], i: usize) -> usize {
    u16::from_le_bytes([b[i], b[i + 1]]) as usize
}
fn u32le(b: &[u8], i: usize) -> usize {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize
}

/// (size, image bytes) for every entry.
fn entries() -> Vec<(usize, &'static [u8])> {
    (0..u16le(ICO, 4))
        .map(|i| {
            let e = 6 + 16 * i;
            let size = if ICO[e] == 0 { 256 } else { ICO[e] as usize };
            (size, &ICO[u32le(ICO, e + 12)..u32le(ICO, e + 12) + u32le(ICO, e + 8)])
        })
        .collect()
}

/// RGBA at (x, y) from a 32-bit BMP icon entry (BGRA rows, bottom-up).
fn bmp_pixel(entry: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
    let i = 40 + ((size - 1 - y) * size + x) * 4;
    [entry[i + 2], entry[i + 1], entry[i], entry[i + 3]]
}

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

#[test]
fn whole_pixel_blocks() {
    let e = entries();
    let (small, double, triple) = (e[0].1, e[1].1, e[2].1);
    for y in 0..48 {
        for x in 0..48 {
            assert_eq!(bmp_pixel(triple, 48, x, y), bmp_pixel(small, 16, x / 3, y / 3));
            if x < 32 && y < 32 {
                assert_eq!(bmp_pixel(double, 32, x, y), bmp_pixel(small, 16, x / 2, y / 2));
                let i = 8 + (y * 32 + x) * 4;
                assert_eq!(&RGBA[i..i + 4], &bmp_pixel(double, 32, x, y));
            }
        }
    }
    assert_eq!((u32le(RGBA, 0), u32le(RGBA, 4), RGBA.len()), (32, 32, 8 + 32 * 32 * 4));
}

#[test]
fn credited() {
    let readme = include_str!("../icon/README.md");
    assert!(readme.contains("Creative Commons Attribution 2.5") && readme.contains("Mark James"));
}

// ==== stamp v1 | author=kjFriday | hmac-sha256=7ecd99000def5e4ba712f160a7a955e5564f292b31e7de6a4b13410a38c4cead ====
