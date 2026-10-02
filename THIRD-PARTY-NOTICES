# Third-Party Software Notices and Licenses

Unarc is open-source software licensed under the MIT License.
This document contains the licenses and notices for third-party software components bundled with or used by Unarc.

---

## 1. Bundled Standalone Engine: 7-Zip (7zz)

Unarc bundles an authentic, pinned build of 7-Zip (`7zz` version 26.03).
7-Zip is developed and copyrighted by Igor Pavlov.

- **Website**: https://www.7-zip.org/
- **Source Code Repository**: https://github.com/ip7z/7zip
- **Version**: 26.03

### Licensing Structure

7-Zip is distributed under the following licensing terms:

1. **GNU LGPL (Lesser General Public License)**:
   The majority of the 7-Zip source code is licensed under the GNU LGPL version 2.1 or later.
2. **unRAR License Restriction**:
   The unRAR portion of the code (handling RAR archives) is governed by the unRAR license, which permits decompression of RAR archives but contains the following restriction:
   > "The unRAR sources may be used in any software to handle RAR archives without limitation on copyright, but the algorithm of RAR compression may not be used to develop any RAR (WinRAR) compatible archiver."
3. **BSD / Public Domain Portions**:
   Certain compression algorithms (including LZMA SDK components and parts of 7z header parsing) are licensed under the BSD 3-Clause License or placed in the Public Domain.

### Architecture & Separation Notice

Unarc does **not** statically or dynamically link to the 7-Zip codebase. Unarc executes the pre-compiled `7zz` binary strictly as an independent subprocess using OS-level process isolation, scrubbed environments, and sandbox boundaries.

---

## 2. Rust Runtime & Library Dependencies

Unarc relies on the following third-party Rust crates:

### MIT License

- `crossterm` (v0.28.1) - Copyright (c) 2018 Timon Post
- `crossterm_winapi` (v0.9.1) - Copyright (c) 2018 Timon Post
- `generic-array` (v0.14.7) - Copyright (c) 2015-2020 Bartłomiej Kamiński, Daniel Keep
- `mio` (v1.2.3) - Copyright (c) 2014 Carl Lerche
- `redox_syscall` (v0.5.18) - Copyright (c) 2017 Redox OS Developers
- `strsim` (v0.11.1) - Copyright (c) 2015 Danny Guo
- `zmij` (v1.0.23) - Copyright (c) 2024 Zmij Developers

### BSD 3-Clause License

- `curve25519-dalek` (v4.1.3) - Copyright (c) 2016-2021 Isis Lovecruft, Henry de Valence
- `ed25519-dalek` (v2.2.0) - Copyright (c) 2016-2021 Isis Lovecruft, Henry de Valence
- `subtle` (v2.6.1) - Copyright (c) 2016-2021 Isis Lovecruft, Henry de Valence

### Apache License 2.0

- `rpassword` (v7.5.4) - Copyright (c) 2014-2022 Conrad Kleinespel
- `rtoolbox` (v0.0.6) - Copyright (c) 2014-2022 Conrad Kleinespel

### Dual-Licensed (MIT OR Apache-2.0)

The following components are dual-licensed under the MIT License and the Apache License, Version 2.0; Unarc utilizes them under the terms of the MIT License:

- `anstream`, `anstyle`, `anstyle-parse`, `anstyle-query`, `anstyle-wincon`
- `bitflags`
- `block-buffer`, `crypto-common`, `digest`, `sha2`
- `cfg-if`
- `clap`, `clap_builder`, `clap_derive`, `clap_lex`
- `colorchoice`
- `cpufeatures`
- `ed25519`
- `errno`
- `getrandom`, `rand_core`
- `heck`
- `is_terminal_polyfill`, `once_cell_polyfill`
- `itoa`
- `libc`, `linux-raw-sys`, `rustix`
- `lock_api`, `parking_lot`, `parking_lot_core`, `smallvec`
- `log`
- `proc-macro2`, `quote`, `syn`
- `scopeguard`
- `semver`, `rustc_version`, `version_check`
- `serde`, `serde_core`, `serde_derive`, `serde_json`
- `signal-hook`, `signal-hook-mio`, `signal-hook-registry`
- `signature`, `pkcs8`, `spki`, `der`, `base64ct`, `const-oid`
- `thiserror`, `thiserror-impl`
- `typenum`
- `zeroize`
- `memchr` (Unlicense OR MIT)
- `unicode-ident` ((MIT OR Apache-2.0) AND Unicode-3.0)

---

## 3. License Texts

### The MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

### BSD 3-Clause License

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors
   may be used to endorse or promote products derived from this software without
   specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED.
IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT,
INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE
OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF
ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

### Apache License 2.0 (Summary)

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
