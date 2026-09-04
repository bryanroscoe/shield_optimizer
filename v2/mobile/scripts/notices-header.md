<!--
  HAND-MAINTAINED SECTION of mobile/THIRD-PARTY-NOTICES.md.

  This file covers everything that is NOT a Cargo dependency: bundled fonts,
  the embedded scrcpy server, the vendored adb_client patch, and the npm
  runtime packages. mobile/scripts/gen-notices.sh concatenates it with the
  cargo-about crate inventory to produce THIRD-PARTY-NOTICES.md and
  src/lib/notices.generated.ts. Edit this file, never the generated ones.
-->
# Third-party notices

ATV Optimizer for Android bundles the third-party software listed below. Each component is used
under the license named with it; the full license texts follow.

Regenerate this document with `mobile/scripts/gen-notices.sh` after any dependency change.

## Components not managed by Cargo

### Geist and Geist Mono — SIL Open Font License 1.1

`src/fonts/Geist-Variable.woff2` and `src/fonts/GeistMono-Variable.woff2` are the variable-weight
WOFF2 builds shipped in the npm `geist` package (1.7.2), unmodified.

Copyright (c) 2023 Vercel, in collaboration with basement.studio

<details>
<summary>SIL Open Font License 1.1 (full text)</summary>

```
Copyright (c) 2023 Vercel, in collaboration with basement.studio

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL

-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION AND CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```

</details>

### Material Symbols Rounded — Apache License 2.0

`src/fonts/material-symbols-rounded.woff2` is a subset of Material Symbols Rounded from the npm
`material-symbols` package (0.45.5), reduced to the glyphs the app actually references by
`mobile/scripts/subset-material-symbols.py`. Subsetting removes glyphs; it does not alter the
outlines that remain.

Copyright Google LLC. Licensed under the Apache License, Version 2.0 — full text reproduced in the
Apache-2.0 section below.

### scrcpy server 3.1 — Apache License 2.0

`scrcpy-server-v3.1` is embedded verbatim in the app binary (`src-tauri/src/scrcpy_resource.rs`,
SHA-256 `958f0944a62f23b1f33a16e9eb14844c1a04b882ca175a738c16d23cb22b86c0`) and pushed to the
connected TV to provide the screen-mirroring stream. It is the unmodified release artifact from
the scrcpy project.

Copyright (C) 2018 Genymobile, Copyright (C) 2018-2024 Romain Vimont. Licensed under the Apache
License, Version 2.0 — full text reproduced in the Apache-2.0 section below.
Source: <https://github.com/Genymobile/scrcpy>

### adb_client — MIT (vendored with modifications)

The wireless-ADB transport is `adb_client` 3.2.2 by Corentin Liaud, vendored at
`v2/vendor/adb_client` and pinned through `[patch.crates-io]`. Shield Optimizer's changes are
described in `v2/vendor/adb_client/SHIELD-OPTIMIZER-PATCH.md`; the crate remains MIT-licensed and
appears in the Cargo inventory below with its upstream license text.

Source: <https://github.com/cocool97/adb_client>

### npm runtime dependencies

Only these npm packages contribute code or assets to the shipped app; everything else in
`package.json` is build-time tooling.

| Package | Version | License |
| --- | --- | --- |
| `svelte` | 5.56.4 | MIT |
| `@tauri-apps/api` | 2.11.1 | Apache-2.0 OR MIT |
| `geist` | 1.7.2 | SIL OFL 1.1 (see above) |
| `material-symbols` | 0.45.5 | Apache-2.0 (see above) |

Svelte compiles into the app bundle; `@tauri-apps/api` ships as JavaScript. Both are used under
the MIT License, whose text is reproduced in the MIT section below.
