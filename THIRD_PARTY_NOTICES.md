# 외부 라이브러리 안내 (Third-Party Notices)

Desktop App for LG K-EXAONE은 아래 오픈소스 라이브러리를 사용합니다. 각 라이브러리의 저작권은 해당 저작자에게 있으며,
각 라이선스 전문은 해당 저장소(또는 설치된 패키지 안의 LICENSE 파일)에서 볼 수 있습니다.

이 파일은 `python scripts/third_party_notices.py`로 만듭니다. 의존성을 바꾸면 다시 실행하세요.

또한 앱이 사용자의 요청에 따라 내려받는 실행 환경(Python, Node.js, uv)과 그 패키지는
앱에 포함되어 배포되지 않으며, 각 공식 배포처의 라이선스를 따릅니다.

- 라이선스를 고를 수 있는 라이브러리는 허용형 쪽을 따릅니다: jszip은 MIT, DOMPurify는 Apache-2.0,
  "MIT OR Apache-2.0" 형식의 Rust 크레이트는 그중 하나를 따릅니다.
- MPL-2.0 라이브러리(cssparser, selectors 등 Tauri가 쓰는 크레이트)는 수정하지 않고 그대로 씁니다.
  각 라이브러리의 소스는 아래 표의 저장소에서 받을 수 있습니다.
- 한글 문서 엔진 @rhwp/core(rhwp, MIT)의 WASM 안에는 rhwp가 함께 묶은 라이브러리(JPEG·JPEG 2000·JBIG2 디코더,
  qcms, svg2pdf, pdfium 등)가 들어 있습니다. 그 저작권 표시와 라이선스 전문은
  `node_modules/@rhwp/core/notices/`와 https://github.com/edwardkim/rhwp 에 있습니다.

## 웹 화면 (npm) — 108개

라이선스 분포: MIT 91개, BSD-2-Clause 5개, ISC 4개, BSD-3-Clause 2개, Apache-2.0 OR MIT 1개, (MPL-2.0 OR Apache-2.0) 1개, BSD 1개, (MIT OR GPL-3.0-or-later) 1개, (MIT AND Zlib) 1개, Apache-2.0 1개

| 이름 | 버전 | 라이선스 | 저장소 |
|---|---|---|---|
| @floating-ui/core | 1.8.0 | MIT | https://github.com/floating-ui/floating-ui |
| @floating-ui/dom | 1.8.0 | MIT | https://github.com/floating-ui/floating-ui |
| @floating-ui/utils | 0.2.12 | MIT | https://github.com/floating-ui/floating-ui |
| @mixmark-io/domino | 2.2.0 | BSD-2-Clause | https://github.com/mixmark-io/domino |
| @napi-rs/canvas | 1.1.1 | MIT | https://github.com/Brooooooklyn/canvas |
| @napi-rs/canvas-win32-x64-msvc | 1.1.1 | MIT | https://github.com/Brooooooklyn/canvas |
| @rhwp/core | 0.9.0 | MIT | https://github.com/edwardkim/rhwp |
| @tauri-apps/api | 2.12.4 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| @tiptap/core | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-blockquote | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-bold | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-bubble-menu | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-bullet-list | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-code | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-code-block | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-document | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-dropcursor | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-floating-menu | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-gapcursor | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-hard-break | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-heading | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-horizontal-rule | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-image | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-italic | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-link | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-list | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-list-item | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-list-keymap | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-ordered-list | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-paragraph | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-strike | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-table | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-text | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extension-underline | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/extensions | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/pm | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/react | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @tiptap/starter-kit | 3.32.0 | MIT | https://github.com/ueberdosis/tiptap |
| @types/node | 22.20.5 | MIT | https://github.com/DefinitelyTyped/DefinitelyTyped |
| @types/node | 26.6.6 | MIT | https://github.com/DefinitelyTyped/DefinitelyTyped |
| @types/react | 19.3.0 | MIT | https://github.com/DefinitelyTyped/DefinitelyTyped |
| @types/react-dom | 19.3.0 | MIT | https://github.com/DefinitelyTyped/DefinitelyTyped |
| @types/trusted-types | 2.0.7 | MIT | https://github.com/DefinitelyTyped/DefinitelyTyped |
| @types/use-sync-external-store | 0.0.6 | MIT | https://github.com/DefinitelyTyped/DefinitelyTyped |
| @xmldom/xmldom | 0.8.15 | MIT | git://github.com/xmldom/xmldom |
| argparse | 1.0.10 | MIT | nodeca/argparse |
| base64-js | 1.5.1 | MIT | git://github.com/beatgammit/base64-js |
| core-util-is | 1.0.3 | MIT | git://github.com/isaacs/core-util-is |
| csstype | 3.2.3 | MIT | https://github.com/frenic/csstype |
| diff | 9.0.0 | BSD-3-Clause | https://github.com/kpdecker/jsdiff |
| dingbat-to-unicode | 1.0.2 | BSD-2-Clause | https://github.com/mwilliamson/dingbat-to-unicode |
| docx | 9.14.0 | MIT | https://github.com/dolanmiu/docx |
| dompurify | 3.4.17 | (MPL-2.0 OR Apache-2.0) | git://github.com/cure53/DOMPurify |
| duck | 0.1.12 | BSD | https://github.com/mwilliamson/duck.js |
| fast-equals | 5.4.5 | MIT | https://github.com/planttheidea/fast-equals |
| hash.js | 1.1.7 | MIT | git@github.com:indutny/hash.js |
| https | 1.0.0 | ISC |  |
| image-size | 1.2.1 | MIT | git://github.com/image-size/image-size |
| immediate | 3.0.6 | MIT | git://github.com/calvinmetcalf/immediate |
| inherits | 2.0.4 | ISC | git://github.com/isaacs/inherits |
| isarray | 1.0.0 | MIT | git://github.com/juliangruber/isarray |
| jszip | 3.10.2 | (MIT OR GPL-3.0-or-later) | https://github.com/Stuk/jszip |
| lie | 3.3.0 | MIT | https://github.com/calvinmetcalf/lie |
| linkifyjs | 4.4.0 | MIT | https://github.com/nfrasser/linkifyjs |
| lop | 0.4.2 | BSD-2-Clause | https://github.com/mwilliamson/lop |
| lucide-react | 0.468.0 | ISC | https://github.com/lucide-icons/lucide |
| mammoth | 1.13.0 | BSD-2-Clause | https://github.com/mwilliamson/mammoth.js |
| marked | 15.0.12 | MIT | git://github.com/markedjs/marked |
| minimalistic-assert | 1.0.1 | ISC | https://github.com/calvinmetcalf/minimalistic-assert |
| nanoid | 6.0.2 | MIT | ai/nanoid |
| option | 0.2.4 | BSD-2-Clause | https://github.com/mwilliamson/node-options |
| orderedmap | 2.1.1 | MIT | https://github.com/marijnh/orderedmap |
| pako | 1.0.11 | (MIT AND Zlib) | nodeca/pako |
| pdfjs-dist | 6.4.299 | Apache-2.0 | https://github.com/mozilla/pdf.js |
| pptxgenjs | 4.0.1 | MIT | https://github.com/gitbrent/PptxGenJS |
| process-nextick-args | 2.0.1 | MIT | https://github.com/calvinmetcalf/process-nextick-args |
| prosemirror-changeset | 2.4.6 | MIT | https://code.haverbeke.berlin/prosemirror/prosemirror-changeset |
| prosemirror-commands | 1.7.2 | MIT | https://code.haverbeke.berlin/prosemirror/prosemirror-commands |
| prosemirror-dropcursor | 1.8.4 | MIT | https://code.haverbeke.berlin/prosemirror/prosemirror-dropcursor |
| prosemirror-gapcursor | 1.4.1 | MIT | git://github.com/prosemirror/prosemirror-gapcursor |
| prosemirror-history | 1.5.1 | MIT | https://code.haverbeke.berlin/prosemirror/prosemirror-history |
| prosemirror-inputrules | 1.6.0 | MIT | https://code.haverbeke.berlin/prosemirror/prosemirror-inputrules |
| prosemirror-keymap | 1.2.3 | MIT | git://github.com/prosemirror/prosemirror-keymap |
| prosemirror-model | 1.25.13 | MIT | https://code.haverbeke.berlin/prosemirror/prosemirror-model |
| prosemirror-schema-list | 1.6.0 | MIT | https://code.haverbeke.berlin/prosemirror/prosemirror-schema-list |
| prosemirror-state | 1.4.4 | MIT | git://github.com/prosemirror/prosemirror-state |
| prosemirror-tables | 1.8.5 | MIT | https://github.com/ProseMirror/prosemirror-tables |
| prosemirror-transform | 1.12.5 | MIT | https://code.haverbeke.berlin/prosemirror/prosemirror-transform |
| prosemirror-view | 1.42.9 | MIT | https://code.haverbeke.berlin/prosemirror/prosemirror-view |
| queue | 6.0.2 | MIT | https://github.com/jessetane/queue |
| react | 19.3.1 | MIT | https://github.com/react/react |
| react-dom | 19.3.1 | MIT | https://github.com/react/react |
| readable-stream | 2.3.8 | MIT | git://github.com/nodejs/readable-stream |
| rope-sequence | 1.3.4 | MIT | https://github.com/marijnh/rope-sequence |
| safe-buffer | 5.1.2 | MIT | git://github.com/feross/safe-buffer |
| scheduler | 0.28.0 | MIT | https://github.com/react/react |
| setimmediate | 1.0.5 | MIT | YuzuJS/setImmediate |
| sprintf-js | 1.0.3 | BSD-3-Clause | https://github.com/alexei/sprintf.js |
| string_decoder | 1.1.1 | MIT | git://github.com/nodejs/string_decoder |
| turndown | 7.2.4 | MIT | https://github.com/mixmark-io/turndown |
| turndown-plugin-gfm | 1.0.2 | MIT | https://github.com/domchristie/turndown-plugin-gfm |
| underscore | 1.13.8 | MIT | git://github.com/jashkenas/underscore |
| undici-types | 6.21.0 | MIT | https://github.com/nodejs/undici |
| undici-types | 8.9.0 | MIT | https://github.com/nodejs/undici |
| use-sync-external-store | 1.7.0 | MIT | https://github.com/react/react |
| util-deprecate | 1.0.2 | MIT | git://github.com/TooTallNate/util-deprecate |
| w3c-keyname | 2.2.8 | MIT | https://github.com/marijnh/w3c-keyname |
| xmlbuilder | 10.1.1 | MIT | git://github.com/oozcitak/xmlbuilder-js |

## 앱 본체 (Rust) — 305개

라이선스 분포: MIT OR Apache-2.0 150개, MIT 58개, Apache-2.0 OR MIT 30개, Unicode-3.0 18개, Unlicense OR MIT 8개, MIT/Apache-2.0 5개, MPL-2.0 5개, BSD-3-Clause 3개, MIT OR Apache-2.0 OR Zlib 3개, Apache-2.0 3개, CC0-1.0 OR MIT-0 OR Apache-2.0 2개, Zlib 2개, Apache-2.0 OR ISC OR MIT 2개, MIT OR Zlib OR Apache-2.0 2개, ISC 2개, Unlicense/MIT 2개, 0BSD OR MIT OR Apache-2.0 1개, BSD-3-Clause AND MIT 1개, BSD-3-Clause/MIT 1개, Apache-2.0 AND MIT 1개, Apache-2.0 / MIT 1개, Apache-2.0 AND ISC 1개, Apache-2.0 OR BSL-1.0 1개, Zlib OR Apache-2.0 OR MIT 1개, (MIT OR Apache-2.0) AND Unicode-3.0 1개, CDLA-Permissive-2.0 1개

| 이름 | 버전 | 라이선스 | 저장소 |
|---|---|---|---|
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | https://github.com/oyvindln/adler2 |
| aho-corasick | 1.1.5 | Unlicense OR MIT | https://github.com/BurntSushi/aho-corasick |
| alloc-no-stdlib | 3.0.0 | BSD-3-Clause | https://github.com/dropbox/rust-alloc-no-stdlib |
| alloc-stdlib | 0.3.0 | BSD-3-Clause | https://github.com/dropbox/rust-alloc-no-stdlib |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | https://github.com/dtolnay/anyhow |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | https://github.com/smol-rs/atomic-waker |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | https://github.com/cuviper/autocfg |
| base64 | 0.22.1 | MIT OR Apache-2.0 | https://github.com/marshallpierce/rust-base64 |
| base64 | 0.23.1 | MIT OR Apache-2.0 | https://github.com/marshallpierce/rust-base64 |
| bit-set | 0.8.0 | Apache-2.0 OR MIT | https://github.com/contain-rs/bit-set |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT | https://github.com/contain-rs/bit-vec |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | https://github.com/bitflags/bitflags |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| brotli | 9.1.1 | BSD-3-Clause AND MIT | https://github.com/dropbox/rust-brotli |
| brotli-decompressor | 6.3.1 | BSD-3-Clause/MIT | https://github.com/dropbox/rust-brotli-decompressor |
| bs58 | 0.5.1 | MIT/Apache-2.0 | https://github.com/Nullus157/bs58-rs |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | https://github.com/fitzgen/bumpalo |
| byteorder | 1.5.0 | Unlicense OR MIT | https://github.com/BurntSushi/byteorder |
| bytes | 1.12.1 | MIT | https://github.com/tokio-rs/bytes |
| camino | 1.2.6 | MIT OR Apache-2.0 | https://github.com/camino-rs/camino |
| cargo-platform | 0.1.9 | MIT OR Apache-2.0 | https://github.com/rust-lang/cargo |
| cargo_metadata | 0.19.2 | MIT | https://github.com/oli-obk/cargo_metadata |
| cargo_toml | 1.0.1 | Apache-2.0 OR MIT | https://gitlab.com/lib.rs/cargo_toml |
| cc | 1.11.2 | MIT OR Apache-2.0 | https://github.com/rust-lang/cc-rs |
| cfb | 0.14.0 | MIT | https://github.com/mdsteele/rust-cfb |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | https://github.com/rust-lang/cfg-if |
| cfg_aliases | 0.2.2 | MIT | https://github.com/katharostech/cfg_aliases |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 | https://github.com/RustCrypto/stream-ciphers |
| chrono | 0.4.45 | MIT OR Apache-2.0 | https://github.com/chronotope/chrono |
| constant_time_eq | 0.6.1 | CC0-1.0 OR MIT-0 OR Apache-2.0 | https://codeberg.org/cesarb/constant_time_eq |
| cookie | 0.18.2 | MIT OR Apache-2.0 | https://github.com/SergioBenitez/cookie-rs |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| crc32fast | 1.5.5 | MIT OR Apache-2.0 | https://github.com/srijs/rust-crc32fast |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| cssparser | 0.37.0 | MPL-2.0 | https://github.com/servo/rust-cssparser |
| cssparser-macros | 0.7.1 | MPL-2.0 | https://github.com/servo/rust-cssparser |
| ctor | 1.0.13 | Apache-2.0 OR MIT | https://github.com/mmastrac/linktime |
| darling | 0.24.1 | MIT | https://github.com/TedDriggs/darling |
| darling_core | 0.24.1 | MIT | https://github.com/TedDriggs/darling |
| darling_macro | 0.24.1 | MIT | https://github.com/TedDriggs/darling |
| deranged | 0.5.9 | MIT OR Apache-2.0 | https://github.com/jhpratt/deranged |
| derive_more | 2.1.1 | MIT | https://github.com/JelteF/derive_more |
| derive_more-impl | 2.1.1 | MIT | https://github.com/JelteF/derive_more |
| digest | 0.10.7 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| dirs | 7.0.0 | MIT OR Apache-2.0 | https://codeberg.org/dirs/dirs-rs |
| dirs-sys | 0.5.0 | MIT OR Apache-2.0 | https://github.com/dirs-dev/dirs-sys-rs |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | https://github.com/yaahc/displaydoc |
| dom_query | 0.28.0 | MIT | https://github.com/niklak/dom_query |
| dpi | 0.1.2 | Apache-2.0 AND MIT | https://github.com/rust-windowing/winit |
| dtoa | 1.0.11 | MIT OR Apache-2.0 | https://github.com/dtolnay/dtoa |
| dtoa-short | 0.3.5 | MPL-2.0 | https://github.com/upsuper/dtoa-short |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | https://gitlab.com/kornelski/dunce |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 | https://github.com/dtolnay/dyn-clone |
| embed-resource | 3.0.12 | MIT | https://github.com/nabijaczleweli/rust-embed-resource |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | https://github.com/indexmap-rs/equivalent |
| erased-serde | 0.4.11 | MIT OR Apache-2.0 | https://github.com/dtolnay/erased-serde |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | https://github.com/smol-rs/fastrand |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | https://github.com/image-rs/fdeflate |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 | https://github.com/rust-lang/cc-rs |
| flate2 | 1.1.11 | MIT OR Apache-2.0 | https://github.com/rust-lang/flate2-rs |
| fnv | 1.0.7 | Apache-2.0 / MIT | https://github.com/servo/rust-fnv |
| foldhash | 0.2.0 | Zlib | https://github.com/orlp/foldhash |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | https://github.com/servo/rust-url |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| generic-array | 0.14.7 | MIT | https://github.com/fizyk20/generic-array.git |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| glob | 0.3.4 | MIT OR Apache-2.0 | https://github.com/rust-lang/glob |
| hashbrown | 0.12.3 | MIT OR Apache-2.0 | https://github.com/rust-lang/hashbrown |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | https://github.com/rust-lang/hashbrown |
| heck | 0.5.0 | MIT OR Apache-2.0 | https://github.com/withoutboats/heck |
| hex | 0.4.3 | MIT OR Apache-2.0 | https://github.com/KokaKiwi/rust-hex |
| html5ever | 0.39.0 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| http | 1.5.0 | MIT OR Apache-2.0 | https://github.com/hyperium/http |
| http-body | 1.1.0 | MIT | https://github.com/hyperium/http-body |
| http-body-util | 0.1.5 | MIT | https://github.com/hyperium/http-body |
| httparse | 1.10.1 | MIT OR Apache-2.0 | https://github.com/seanmonstar/httparse |
| hyper | 1.12.0 | MIT | https://github.com/hyperium/hyper |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT | https://github.com/rustls/hyper-rustls |
| hyper-util | 0.1.21 | MIT | https://github.com/hyperium/hyper-util |
| ico | 0.6.0 | MIT | https://github.com/mdsteele/rust-ico |
| icu_collections | 2.4.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_locale_core | 2.4.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_normalizer | 2.4.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_properties | 2.4.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_properties_data | 2.4.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| icu_provider | 2.4.0 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| ident_case | 1.0.1 | MIT/Apache-2.0 | https://github.com/TedDriggs/ident_case |
| idna | 1.1.0 | MIT OR Apache-2.0 | https://github.com/servo/rust-url/ |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | https://github.com/hsivonen/idna_adapter |
| indexmap | 1.9.3 | Apache-2.0 OR MIT | https://github.com/bluss/indexmap |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | https://github.com/indexmap-rs/indexmap |
| infer | 0.22.0 | MIT | https://github.com/bojand/infer |
| ipnet | 2.12.2 | MIT OR Apache-2.0 | https://github.com/krisprice/ipnet |
| itoa | 1.0.19 | MIT OR Apache-2.0 | https://github.com/dtolnay/itoa |
| jiff | 0.2.43 | Unlicense OR MIT | https://github.com/BurntSushi/jiff |
| jiff-core | 0.1.5 | Unlicense OR MIT | https://github.com/BurntSushi/jiff |
| jiff-tzdb | 0.1.9 | Unlicense OR MIT | https://github.com/BurntSushi/jiff |
| jiff-tzdb-platform | 0.1.3 | Unlicense OR MIT | https://github.com/BurntSushi/jiff |
| json-patch | 4.2.0 | MIT/Apache-2.0 | https://github.com/idubrov/json-patch |
| jsonptr | 0.7.1 | MIT OR Apache-2.0 | https://github.com/chanced/jsonptr |
| keyboard-types | 0.8.3 | MIT OR Apache-2.0 | https://github.com/rust-windowing/keyboard-types |
| keyring | 3.6.3 | MIT OR Apache-2.0 | https://github.com/hwchen/keyring-rs.git |
| libc | 0.2.191 | MIT OR Apache-2.0 | https://github.com/rust-lang/libc |
| litemap | 0.8.3 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| log | 0.4.35 | MIT OR Apache-2.0 | https://github.com/rust-lang/log |
| lru-slab | 0.1.3 | MIT OR Apache-2.0 OR Zlib | https://github.com/Ralith/lru-slab |
| markup5ever | 0.39.0 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| memchr | 2.8.3 | Unlicense OR MIT | https://github.com/BurntSushi/memchr |
| mime | 0.3.18 | MIT OR Apache-2.0 | https://github.com/hyperium/mime |
| minisign-verify | 0.3.0 | MIT | https://github.com/jedisct1/rust-minisign-verify |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| mio | 1.2.4 | MIT | https://github.com/tokio-rs/mio |
| muda | 0.20.0 | Apache-2.0 OR MIT | https://github.com/tauri-apps/muda |
| new_debug_unreachable | 1.0.6 | MIT | https://github.com/mbrubeck/rust-debug-unreachable |
| num-conv | 0.2.2 | MIT OR Apache-2.0 | https://github.com/jhpratt/num-conv |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | https://github.com/rust-num/num-traits |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | https://github.com/matklad/once_cell |
| option-ext | 0.2.0 | MPL-2.0 | https://github.com/soc/option-ext.git |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | https://github.com/servo/rust-url/ |
| phf | 0.13.1 | MIT | https://github.com/rust-phf/rust-phf |
| phf_codegen | 0.13.1 | MIT | https://github.com/rust-phf/rust-phf |
| phf_generator | 0.13.1 | MIT | https://github.com/rust-phf/rust-phf |
| phf_macros | 0.13.1 | MIT | https://github.com/rust-phf/rust-phf |
| phf_shared | 0.13.1 | MIT | https://github.com/rust-phf/rust-phf |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | https://github.com/taiki-e/pin-project-lite |
| plist | 1.10.3 | MIT | https://github.com/ebarnard/rust-plist/ |
| png | 0.18.1 | MIT OR Apache-2.0 | https://github.com/image-rs/image-png |
| potential_utf | 0.1.6 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 | https://github.com/jhpratt/powerfmt |
| precomputed-hash | 0.1.1 | MIT | https://github.com/emilio/precomputed-hash |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | https://github.com/dtolnay/proc-macro2 |
| quick-xml | 0.42.0 | MIT | https://github.com/tafia/quick-xml |
| quinn | 0.11.12 | MIT OR Apache-2.0 | https://github.com/quinn-rs/quinn |
| quinn-proto | 0.11.19 | MIT OR Apache-2.0 | https://github.com/quinn-rs/quinn |
| quinn-udp | 0.5.16 | MIT OR Apache-2.0 | https://github.com/quinn-rs/quinn |
| quote | 1.0.47 | MIT OR Apache-2.0 | https://github.com/dtolnay/quote |
| rand | 0.10.3 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | https://github.com/rust-random/rand_core |
| rand_pcg | 0.10.2 | MIT OR Apache-2.0 | https://github.com/rust-random/rngs |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | https://github.com/rust-windowing/raw-window-handle |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 | https://github.com/dtolnay/ref-cast |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 | https://github.com/dtolnay/ref-cast |
| regex | 1.13.1 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| reqwest | 0.12.28 | MIT OR Apache-2.0 | https://github.com/seanmonstar/reqwest |
| reqwest | 0.13.5 | MIT OR Apache-2.0 | https://github.com/seanmonstar/reqwest |
| ring | 0.17.14 | Apache-2.0 AND ISC | https://github.com/briansmith/ring |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | https://github.com/rust-lang/rustc-hash |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | https://github.com/djc/rustc-version-rs |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | https://github.com/rustls/rustls |
| rustls-pki-types | 1.16.0 | MIT OR Apache-2.0 | https://github.com/rustls/pki-types |
| rustls-platform-verifier | 0.7.1 | MIT OR Apache-2.0 | https://github.com/rustls/rustls-platform-verifier |
| rustls-webpki | 0.103.17 | ISC | https://github.com/rustls/webpki |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | https://github.com/dtolnay/rustversion |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | https://github.com/dtolnay/ryu |
| same-file | 1.0.6 | Unlicense/MIT | https://github.com/BurntSushi/same-file |
| schemars | 0.8.22 | MIT | https://github.com/GREsau/schemars |
| schemars | 0.9.0 | MIT | https://github.com/GREsau/schemars |
| schemars | 1.2.3 | MIT | https://github.com/GREsau/schemars |
| schemars_derive | 0.8.22 | MIT | https://github.com/GREsau/schemars |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | https://github.com/bluss/scopeguard |
| selectors | 0.38.0 | MPL-2.0 | https://github.com/servo/stylo |
| semver | 1.0.28 | MIT OR Apache-2.0 | https://github.com/dtolnay/semver |
| serde | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde-untagged | 0.1.9 | MIT OR Apache-2.0 | https://github.com/dtolnay/serde-untagged |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde_derive_internals | 0.29.1 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | https://github.com/serde-rs/json |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | https://github.com/dtolnay/serde-repr |
| serde_spanned | 1.1.3 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 | https://github.com/nox/serde_urlencoded |
| serde_with | 3.24.0 | MIT OR Apache-2.0 | https://github.com/jonasbb/serde_with/ |
| serde_with_macros | 3.24.0 | MIT OR Apache-2.0 | https://github.com/jonasbb/serde_with/ |
| serialize-to-javascript | 0.1.2 | MIT OR Apache-2.0 | https://github.com/chippers/serialize-to-javascript |
| serialize-to-javascript-impl | 0.1.2 | MIT OR Apache-2.0 | https://github.com/chippers/serialize-to-javascript |
| servo_arc | 0.4.3 | MIT OR Apache-2.0 | https://github.com/servo/stylo |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| shlex | 2.0.1 | MIT OR Apache-2.0 | https://github.com/comex/rust-shlex |
| simd-adler32 | 0.3.10 | MIT | https://github.com/mcountryman/simd-adler32 |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | https://github.com/jedisct1/rust-siphash |
| slab | 0.4.12 | MIT | https://github.com/tokio-rs/slab |
| smallvec | 1.16.5 | MIT OR Apache-2.0 | https://github.com/servo/rust-smallvec |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | https://github.com/rust-lang/socket2 |
| softbuffer | 0.4.8 | MIT OR Apache-2.0 | https://github.com/rust-windowing/softbuffer |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | https://github.com/storyyeller/stable_deref_trait |
| static_vcruntime | 3.0.0 | MIT OR Apache-2.0 OR Zlib | https://github.com/ChrisDenton/static_vcruntime |
| string_cache | 0.9.0 | MIT OR Apache-2.0 | https://github.com/servo/string-cache |
| string_cache_codegen | 0.6.1 | MIT OR Apache-2.0 | https://github.com/servo/string-cache |
| strsim | 0.11.1 | MIT | https://github.com/rapidfuzz/strsim-rs |
| subtle | 2.6.1 | BSD-3-Clause | https://github.com/dalek-cryptography/subtle |
| syn | 2.0.119 | MIT OR Apache-2.0 | https://github.com/dtolnay/syn |
| syn | 3.0.10 | MIT OR Apache-2.0 | https://github.com/dtolnay/syn |
| sync_wrapper | 1.0.2 | Apache-2.0 | https://github.com/Actyx/sync_wrapper |
| synstructure | 0.14.0 | MIT | https://github.com/mystor/synstructure |
| tao | 0.37.2 | Apache-2.0 | https://github.com/tauri-apps/tao |
| tauri | 2.12.5 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| tauri-build | 2.7.4 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| tauri-codegen | 2.7.4 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| tauri-macros | 2.7.4 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| tauri-plugin | 2.7.2 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| tauri-plugin-updater | 2.14.1 | Apache-2.0 OR MIT | https://github.com/tauri-apps/plugins-workspace |
| tauri-runtime | 2.12.2 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| tauri-runtime-wry | 2.12.3 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| tauri-utils | 2.10.2 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| tauri-winres | 0.3.7 | MIT | https://github.com/tauri-apps/winres |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | https://github.com/Stebalien/tempfile |
| tendril | 0.5.1 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| time | 0.3.55 | MIT OR Apache-2.0 | https://github.com/time-rs/time |
| time-core | 0.1.9 | MIT OR Apache-2.0 | https://github.com/time-rs/time |
| time-macros | 0.2.32 | MIT OR Apache-2.0 | https://github.com/time-rs/time |
| tinystr | 0.8.5 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | https://github.com/Lokathor/tinyvec |
| tokio | 1.54.1 | MIT | https://github.com/tokio-rs/tokio |
| tokio-macros | 2.7.2 | MIT | https://github.com/tokio-rs/tokio |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 | https://github.com/rustls/tokio-rustls |
| tokio-util | 0.7.20 | MIT | https://github.com/tokio-rs/tokio |
| toml | 1.2.1+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_datetime | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_writer | 1.2.0+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| tower | 0.5.3 | MIT | https://github.com/tower-rs/tower |
| tower-http | 0.6.11 | MIT | https://github.com/tower-rs/tower-http |
| tower-layer | 0.3.3 | MIT | https://github.com/tower-rs/tower |
| tower-service | 0.3.3 | MIT | https://github.com/tower-rs/tower |
| tracing | 0.1.44 | MIT | https://github.com/tokio-rs/tracing |
| tracing-core | 0.1.36 | MIT | https://github.com/tokio-rs/tracing |
| trash | 5.3.0 | MIT | https://github.com/ArturKovacs/trash |
| tray-icon | 0.25.1 | MIT OR Apache-2.0 | https://github.com/tauri-apps/tray-icon |
| try-lock | 0.2.5 | MIT | https://github.com/seanmonstar/try-lock |
| typed-path | 0.13.0 | MIT OR Apache-2.0 | https://github.com/chipsenkbeil/typed-path |
| typeid | 1.0.3 | MIT OR Apache-2.0 | https://github.com/dtolnay/typeid |
| typenum | 1.20.1 | MIT OR Apache-2.0 | https://github.com/paholg/typenum |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | https://github.com/dtolnay/unicode-ident |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | https://github.com/unicode-rs/unicode-segmentation |
| untrusted | 0.9.0 | ISC | https://github.com/briansmith/untrusted |
| url | 2.5.8 | MIT OR Apache-2.0 | https://github.com/servo/rust-url |
| urlpattern | 0.6.0 | MIT | https://github.com/denoland/rust-urlpattern |
| utf8_iter | 1.1.0 | Apache-2.0 OR MIT | https://github.com/hsivonen/utf8_iter |
| utf_types | 1.0.0 | Apache-2.0 OR MIT | https://github.com/hsivonen/utf_types |
| uuid | 1.29.2 | Apache-2.0 OR MIT | https://github.com/uuid-rs/uuid |
| version_check | 0.9.5 | MIT/Apache-2.0 | https://github.com/SergioBenitez/version_check |
| walkdir | 2.5.0 | Unlicense/MIT | https://github.com/BurntSushi/walkdir |
| want | 0.3.2 | MIT | https://github.com/seanmonstar/want |
| web-time | 1.1.0 | MIT OR Apache-2.0 | https://github.com/daxpedda/web-time |
| web_atoms | 0.2.6 | MIT OR Apache-2.0 | https://github.com/servo/html5ever |
| webpki-roots | 1.0.12 | CDLA-Permissive-2.0 | https://github.com/rustls/webpki-roots |
| webview2-com | 0.39.1 | MIT | https://github.com/wravery/webview2-rs |
| webview2-com-macros | 0.8.1 | MIT | https://github.com/wravery/webview2-rs |
| webview2-com-sys | 0.39.1 | MIT | https://github.com/wravery/webview2-rs |
| winapi-util | 0.1.11 | Unlicense OR MIT | https://github.com/BurntSushi/winapi-util |
| window-vibrancy | 0.8.1 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri-plugin-vibrancy |
| windows | 0.62.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-future | 0.3.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-registry | 0.6.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-sys | 0.60.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-targets | 0.53.5 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-version | 0.1.7 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnu | 0.53.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| winnow | 1.0.5 | MIT | https://github.com/winnow-rs/winnow |
| writeable | 0.6.4 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| wry | 0.57.2 | Apache-2.0 OR MIT | https://github.com/tauri-apps/wry |
| yoke | 0.8.4 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| yoke-derive | 0.8.4 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerofrom | 0.1.8 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | https://github.com/RustCrypto/utils |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT | https://github.com/RustCrypto/utils |
| zerotrie | 0.2.5 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerovec | 0.11.9 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zerovec-derive | 0.11.7 | Unicode-3.0 | https://github.com/unicode-org/icu4x |
| zip | 2.6.1 | MIT | https://github.com/zip-rs/zip2.git |
| zip | 9.6.0 | MIT | https://github.com/zip-rs/zip2 |
| zlib-rs | 0.6.8 | Zlib | https://github.com/trifectatechfoundation/zlib-rs |
| zmij | 1.0.23 | MIT | https://github.com/dtolnay/zmij |
| zopfli | 0.8.4 | Apache-2.0 | https://github.com/zopfli-rs/zopfli |
