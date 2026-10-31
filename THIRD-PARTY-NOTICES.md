# Third-party notices

FreeMic is MIT licensed. It is built from the open-source Rust crates listed below,
each under its own license (identifier shown; full texts are in each crate's source).

* **Symphonia** (MPL-2.0) is used unmodified; its source code is available at https://github.com/pdeljanov/Symphonia.
* egui's bundled fonts: Ubuntu font (Ubuntu Font Licence 1.0), Hack and emoji fonts (OFL-1.1 / MIT / Apache-2.0).
* **Inter** typeface 4.1 (SIL Open Font License 1.1), Copyright (c) 2016 The Inter Project Authors, https://github.com/rsms/inter. Embedded unmodified; the license text is in `assets/fonts/Inter-LICENSE.txt`.
* **yt-dlp** (https://github.com/yt-dlp/yt-dlp) runs as a separate program. **Windows packages include** the official, unmodified `yt-dlp.exe`. yt-dlp itself is released under the Unlicense; its PyInstaller-built standalone executable also contains Python and other components, and as a whole is licensed under the **GNU GPL v3 or later**. Its license texts ship next to it (`yt-dlp-LICENSE.txt`, `yt-dlp-THIRD_PARTY_LICENSES.txt`) together with `yt-dlp-SOURCE.txt`, which says where its source code is. FreeMic only starts it as a separate program and passes it command-line arguments, so FreeMic itself remains MIT licensed. On Linux, yt-dlp is not included: FreeMic downloads the official `yt-dlp_linux` from yt-dlp's releases when you first use the YouTube page.
* The YouTube integration ports the search and download logic of YT-Downloader (https://github.com/VVVULTURE/YT-Downloader) by the same author.
* On Linux FreeMic links dynamically to the system's libpulse (LGPL-2.1+).
* VB-CABLE is **not** included, redistributed or downloaded by FreeMic. It is a separate product of VB-Audio Software (donationware) that you install from vb-audio.com.

| Crate | Version | License | Platforms | Source |
|---|---|---|---|---|
| accesskit | 0.25.2 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/AccessKit/accesskit |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | Linux, Windows | https://github.com/oyvindln/adler2 |
| ahash | 0.8.12 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/tkaitchuck/ahash |
| arboard | 3.7.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/1Password/arboard |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/bluss/arrayvec |
| as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 | Linux | https://github.com/psychon/as-raw-xcb-connection |
| audio-codec-algorithms | 0.8.1 | 0BSD OR Apache-2.0 | Linux, Windows | https://github.com/karip/audio-codec-algorithms |
| audioadapter | 5.0.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/HEnquist/audioadapter-rs |
| audioadapter-buffers | 5.2.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/HEnquist/audioadapter-buffers-rs |
| audioadapter-sample | 5.2.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/HEnquist/audioadapter-sample-rs |
| base64 | 0.23.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/marshallpierce/rust-base64 |
| bit-set | 0.10.0 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/contain-rs/bit-set |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/contain-rs/bit-vec |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/bitflags/bitflags |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/RustCrypto/utils |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | Linux, Windows | https://github.com/Lokathor/bytemuck |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | Linux, Windows | https://github.com/Lokathor/bytemuck |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | Linux, Windows | https://github.com/image-rs/byteorder-lite |
| bytes | 1.12.1 | MIT | Linux, Windows | https://github.com/tokio-rs/bytes |
| calloop | 0.13.0 | MIT | Linux | https://github.com/Smithay/calloop |
| calloop-wayland-source | 0.3.0 | MIT | Linux | https://github.com/smithay/calloop-wayland-source |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-lang/cfg-if |
| clipboard-win | 5.4.3 | BSL-1.0 | Windows | https://github.com/DoumanAsh/clipboard-win |
| codespan-reporting | 0.13.1 | Apache-2.0 | Linux, Windows | https://github.com/brendanzab/codespan |
| color | 0.3.3 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/linebender/color |
| const-oid | 0.10.2 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/RustCrypto/formats |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/RustCrypto/utils |
| crc32fast | 1.5.3 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/srijs/rust-crc32fast |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/crossbeam-rs/crossbeam |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/RustCrypto/traits |
| cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib | Linux, Windows | https://github.com/rust-windowing/cursor-icon |
| digest | 0.11.3 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/RustCrypto/traits |
| dlib | 0.5.3 | MIT | Linux | https://github.com/elinorbgr/dlib |
| document-features | 0.2.12 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/slint-ui/document-features |
| downcast-rs | 1.2.1 | MIT/Apache-2.0 | Linux | https://github.com/marcianx/downcast-rs |
| dpi | 0.1.2 | Apache-2.0 AND MIT | Linux, Windows | https://github.com/rust-windowing/winit |
| ecolor | 0.37.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/emilk/egui |
| eframe | 0.37.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/emilk/egui/tree/main/crates/eframe |
| egui | 0.37.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/emilk/egui |
| egui-wgpu | 0.37.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/emilk/egui/tree/main/crates/egui-wgpu |
| egui-winit | 0.37.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/emilk/egui/tree/main/crates/egui-winit |
| egui_glow | 0.37.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/emilk/egui/tree/main/crates/egui_glow |
| either | 1.19.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rayon-rs/either |
| emath | 0.37.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/emilk/egui/tree/main/crates/emath |
| epaint | 0.37.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/emilk/egui/tree/main/crates/epaint |
| epaint_default_fonts | 0.37.0 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 | Linux, Windows | https://github.com/emilk/egui/tree/main/crates/epaint_default_fonts |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/indexmap-rs/equivalent |
| errno | 0.3.14 | MIT OR Apache-2.0 | Linux | https://github.com/lambda-fairy/rust-errno |
| error-code | 3.4.0 | BSL-1.0 | Windows | https://github.com/DoumanAsh/error-code |
| euclid | 0.22.14 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/servo/euclid |
| extended | 0.1.0 | MIT | Linux, Windows | https://github.com/depp/extended-rs |
| fax | 0.2.7 | MIT | Linux, Windows | https://github.com/pdf-rs/fax |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/image-rs/fdeflate |
| fearless_simd | 0.4.1 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/linebender/fearless_simd |
| fixedbitset | 0.5.7 | MIT OR Apache-2.0 | Linux | https://github.com/petgraph/fixedbitset |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-lang/flate2-rs |
| foldhash | 0.1.5 | Zlib | Linux | https://github.com/orlp/foldhash |
| foldhash | 0.2.0 | Zlib | Linux, Windows | https://github.com/orlp/foldhash |
| font-types | 0.12.7 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/googlefonts/fontations |
| gethostname | 1.1.0 | Apache-2.0 | Linux | https://codeberg.org/swsnr/gethostname.rs.git |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-random/getrandom |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-random/getrandom |
| glifo | 0.3.0 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/linebender/vello |
| global-hotkey | 0.9.0 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/tauri-apps/global-hotkey |
| glow | 0.17.0 | MIT OR Apache-2.0 OR Zlib | Linux, Windows | https://github.com/grovesNL/glow |
| glutin | 0.32.3 | Apache-2.0 | Linux, Windows | https://github.com/rust-windowing/glutin |
| glutin-winit | 0.5.0 | MIT | Linux, Windows | https://github.com/rust-windowing/glutin |
| glutin_egl_sys | 0.7.1 | Apache-2.0 | Linux, Windows | https://github.com/rust-windowing/glutin |
| glutin_glx_sys | 0.6.1 | Apache-2.0 | Linux | https://github.com/rust-windowing/glutin |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 | Windows | https://github.com/rust-windowing/glutin |
| guillotiere | 0.7.0 | MIT/Apache-2.0 | Linux, Windows | https://github.com/nical/guillotiere |
| half | 2.7.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/VoidStarKat/half-rs |
| harfrust | 0.12.0 | MIT | Linux, Windows | https://github.com/harfbuzz/harfrust |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 | Linux | https://github.com/rust-lang/hashbrown |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-lang/hashbrown |
| http | 1.5.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/hyperium/http |
| httparse | 1.10.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/seanmonstar/httparse |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/RustCrypto/hybrid-array |
| image | 0.25.10 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/image-rs/image |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/indexmap-rs/indexmap |
| itertools | 0.15.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-itertools/itertools |
| itoa | 1.0.18 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/dtolnay/itoa |
| keyboard-types | 0.8.3 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-windowing/keyboard-types |
| kurbo | 0.13.1 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/linebender/kurbo |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-lang-nursery/lazy-static.rs |
| libc | 0.2.191 | MIT OR Apache-2.0 | Linux | https://github.com/rust-lang/libc |
| libloading | 0.8.9 | ISC | Linux, Windows | https://github.com/nagisa/rust_libloading/ |
| libm | 0.2.16 | MIT | Linux, Windows | https://github.com/rust-lang/compiler-builtins |
| libpulse-binding | 2.30.1 | MIT OR Apache-2.0 | Linux | https://github.com/jnqnfe/pulse-binding-rust |
| libpulse-sys | 1.23.0 | MIT OR Apache-2.0 | Linux | https://github.com/jnqnfe/pulse-binding-rust |
| linebender_resource_handle | 0.1.1 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/linebender/raw_resource_handle |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Linux | https://github.com/sunfishcode/linux-raw-sys |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Linux | https://github.com/sunfishcode/linux-raw-sys |
| litrs | 1.0.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/LukasKalbertodt/litrs |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/Amanieu/parking_lot |
| log | 0.4.34 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-lang/log |
| memchr | 2.8.3 | Unlicense OR MIT | Linux, Windows | https://github.com/BurntSushi/memchr |
| memmap2 | 0.9.12 | MIT OR Apache-2.0 | Linux | https://github.com/RazrFalcon/memmap2-rs |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | Linux, Windows | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | Linux, Windows | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | Linux, Windows | https://github.com/awxkee/moxcms.git |
| naga | 30.0.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/gfx-rs/wgpu |
| naga-types | 30.0.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/gfx-rs/wgpu |
| nohash-hasher | 0.2.0 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/paritytech/nohash-hasher |
| nom | 8.0.0 | MIT | Linux | https://github.com/rust-bakery/nom |
| num-complex | 0.4.6 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-num/num-complex |
| num-derive | 0.4.2 | MIT OR Apache-2.0 | Linux | https://github.com/rust-num/num-derive |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-num/num-integer |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-num/num-traits |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/matklad/once_cell |
| os_pipe | 1.2.3 | MIT | Linux | https://github.com/oconnor663/os_pipe.rs |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/Amanieu/parking_lot |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/Amanieu/parking_lot |
| peniko | 0.6.1 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/linebender/peniko |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/servo/rust-url/ |
| petgraph | 0.8.3 | MIT OR Apache-2.0 | Linux | https://github.com/petgraph/petgraph |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/taiki-e/pin-project-lite |
| png | 0.18.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/image-rs/image-png |
| polling | 3.11.0 | Apache-2.0 OR MIT | Linux | https://github.com/smol-rs/polling |
| pollster | 0.4.0 | Apache-2.0/MIT | Linux | https://github.com/zesterer/pollster |
| polycool | 0.4.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/linebender/kurbo |
| primal-check | 0.3.4 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/huonw/primal |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/dtolnay/proc-macro2 |
| profiling | 1.0.18 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/aclysma/profiling |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | Linux, Windows | https://github.com/awxkee/pxfm |
| quick-error | 2.0.1 | MIT/Apache-2.0 | Linux, Windows | http://github.com/tailhook/quick-error |
| quick-xml | 0.41.0 | MIT | Linux | https://github.com/tafia/quick-xml |
| quote | 1.0.47 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/dtolnay/quote |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | Linux, Windows | https://github.com/rust-windowing/raw-window-handle |
| read-fonts | 0.41.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/googlefonts/fontations |
| realfft | 3.5.1 | MIT | Linux, Windows | https://github.com/HEnquist/realfft |
| regex-lite | 0.1.9 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-lang/regex |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/ebkalderon/renderdoc-rs |
| rfd | 0.17.2 | MIT | Linux, Windows | https://github.com/PolyMeilex/rfd |
| ring | 0.17.14 | Apache-2.0 AND ISC | Linux, Windows | https://github.com/briansmith/ring |
| rtrb | 0.4.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/mgeier/rtrb |
| rubato | 5.0.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/HEnquist/rubato |
| rustc-hash | 1.1.0 | Apache-2.0/MIT | Linux, Windows | https://github.com/rust-lang-nursery/rustc-hash |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/rust-lang/rustc-hash |
| rustfft | 6.4.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/ejmahler/RustFFT |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Linux | https://github.com/bytecodealliance/rustix |
| rustix | 1.1.6 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Linux | https://github.com/bytecodealliance/rustix |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | Linux, Windows | https://github.com/rustls/rustls |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rustls/pki-types |
| rustls-webpki | 0.103.16 | ISC | Linux, Windows | https://github.com/rustls/webpki |
| scoped-tls | 1.0.1 | MIT/Apache-2.0 | Linux | https://github.com/alexcrichton/scoped-tls |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/bluss/scopeguard |
| self_cell | 1.3.0 | Apache-2.0 OR GPL-2.0-only | Linux, Windows | https://github.com/Voultapher/self_cell |
| serde | 1.0.229 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/serde-rs/serde |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/serde-rs/serde |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/serde-rs/serde |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/serde-rs/json |
| sha2 | 0.11.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/RustCrypto/hashes |
| simd-adler32 | 0.3.10 | MIT | Linux, Windows | https://github.com/mcountryman/simd-adler32 |
| skrifa | 0.44.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/googlefonts/fontations |
| slab | 0.4.12 | MIT | Linux | https://github.com/tokio-rs/slab |
| smallvec | 1.16.4 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/servo/rust-smallvec |
| smithay-client-toolkit | 0.19.2 | MIT | Linux | https://github.com/smithay/client-toolkit |
| smol_str | 0.2.2 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/rust-analyzer/smol_str |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/nvzqz/static-assertions-rs |
| strength_reduce | 0.2.4 | MIT OR Apache-2.0 | Linux, Windows | http://github.com/ejmahler/strength_reduce |
| subtle | 2.6.1 | BSD-3-Clause | Linux, Windows | https://github.com/dalek-cryptography/subtle |
| symphonia | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-bundle-flac | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-bundle-mp3 | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-codec-aac | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-codec-adpcm | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-codec-alac | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-codec-pcm | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-codec-vorbis | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-common | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-core | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-format-isomp4 | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-format-ogg | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-format-riff | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| symphonia-metadata | 0.6.1 | MPL-2.0 | Linux, Windows | https://github.com/pdeljanov/Symphonia |
| syn | 2.0.119 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/dtolnay/syn |
| syn | 3.0.9 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/dtolnay/syn |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | Linux | https://github.com/dtolnay/thiserror |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/dtolnay/thiserror |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | Linux | https://github.com/dtolnay/thiserror |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/dtolnay/thiserror |
| tiff | 0.11.3 | MIT | Linux, Windows | https://github.com/image-rs/image-tiff |
| tracing | 0.1.44 | MIT | Linux, Windows | https://github.com/tokio-rs/tracing |
| tracing-core | 0.1.36 | MIT | Linux, Windows | https://github.com/tokio-rs/tracing |
| transpose | 0.2.3 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/ejmahler/transpose |
| tree_magic_mini | 3.2.2 | MIT | Linux | https://github.com/mbrubeck/tree_magic/ |
| type-map | 0.5.1 | MIT/Apache-2.0 | Linux, Windows | https://github.com/kardeiz/type-map |
| typenum | 1.20.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/paholg/typenum |
| unicode-general-category | 1.1.0 | Apache-2.0 | Linux, Windows | https://github.com/yeslogic/unicode-general-category |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | Linux, Windows | https://github.com/dtolnay/unicode-ident |
| unicode-properties | 0.1.4 | MIT/Apache-2.0 | Linux, Windows | https://github.com/unicode-rs/unicode-properties |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/unicode-rs/unicode-segmentation |
| unicode-width | 0.2.3 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/unicode-rs/unicode-width |
| untrusted | 0.9.0 | ISC | Linux, Windows | https://github.com/briansmith/untrusted |
| ureq | 3.5.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/algesten/ureq |
| ureq-proto | 0.6.5 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/algesten/ureq-proto |
| utf8-zero | 0.8.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/algesten/utf8-zero |
| uuid | 1.29.0 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/uuid-rs/uuid |
| vello_common | 0.2.0 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/linebender/vello |
| vello_cpu | 0.2.0 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/linebender/vello |
| visibility | 0.1.1 | Zlib OR MIT OR Apache-2.0 | Linux, Windows | https://github.com/danielhenrymantilla/visibility.rs |
| wasapi | 0.26.0 | MIT | Windows | https://github.com/HEnquist/wasapi-rs |
| wayland-backend | 0.3.17 | MIT | Linux | https://github.com/smithay/wayland-rs |
| wayland-client | 0.31.15 | MIT | Linux | https://github.com/smithay/wayland-rs |
| wayland-csd-frame | 0.3.0 | MIT | Linux | https://github.com/rust-windowing/wayland-csd-frame |
| wayland-cursor | 0.31.14 | MIT | Linux | https://github.com/smithay/wayland-rs |
| wayland-protocols | 0.32.13 | MIT | Linux | https://github.com/smithay/wayland-rs |
| wayland-protocols-plasma | 0.3.12 | MIT | Linux | https://github.com/smithay/wayland-rs |
| wayland-protocols-wlr | 0.3.12 | MIT | Linux | https://github.com/smithay/wayland-rs |
| wayland-scanner | 0.31.11 | MIT | Linux | https://github.com/smithay/wayland-rs |
| wayland-sys | 0.31.11 | MIT | Linux | https://github.com/smithay/wayland-rs |
| web-time | 1.1.0 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/daxpedda/web-time |
| webpki-roots | 1.0.11 | CDLA-Permissive-2.0 | Linux, Windows | https://github.com/rustls/webpki-roots |
| weezl | 0.1.12 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/image-rs/weezl |
| wgpu | 30.0.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/gfx-rs/wgpu |
| wgpu-core | 30.0.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/gfx-rs/wgpu |
| wgpu-core-deps-windows-linux-android | 30.0.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/gfx-rs/wgpu |
| wgpu-hal | 30.0.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/gfx-rs/wgpu |
| wgpu-naga-bridge | 30.0.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/gfx-rs/wgpu |
| wgpu-types | 30.0.1 | MIT OR Apache-2.0 | Linux, Windows | https://github.com/gfx-rs/wgpu |
| windowfunctions | 0.1.1 | MIT | Linux, Windows | https://github.com/HEnquist/windowfunctions-rs |
| windows | 0.62.2 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-future | 0.3.2 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | Windows | https://github.com/microsoft/windows-rs |
| winit | 0.30.13 | Apache-2.0 | Linux, Windows | https://github.com/rust-windowing/winit |
| wl-clipboard-rs | 0.9.5 | MIT OR Apache-2.0 | Linux | https://github.com/YaLTeR/wl-clipboard-rs |
| x11-dl | 2.21.0 | MIT | Linux | https://github.com/AltF02/x11-rs.git |
| x11rb | 0.13.2 | MIT OR Apache-2.0 | Linux | https://github.com/psychon/x11rb |
| x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 | Linux | https://github.com/psychon/x11rb |
| xcursor | 0.3.11 | MIT | Linux | https://github.com/esposm03/xcursor-rs |
| xkbcommon-dl | 0.4.2 | MIT | Linux | https://github.com/rust-windowing/xkbcommon-dl |
| xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib | Linux | https://github.com/notgull/xkeysym |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | Linux, Windows | https://github.com/google/zerocopy |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | Linux, Windows | https://github.com/google/zerocopy |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | Linux, Windows | https://github.com/RustCrypto/utils |
| zlib-rs | 0.6.8 | Zlib | Linux, Windows | https://github.com/trifectatechfoundation/zlib-rs |
| zmij | 1.0.23 | MIT | Linux, Windows | https://github.com/dtolnay/zmij |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | Linux, Windows | https://github.com/etemesi254/zune-image |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | Linux, Windows | https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg |
