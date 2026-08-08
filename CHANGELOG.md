## [1.9.1](https://github.com/harubi/bolivar/compare/v1.9.0...v1.9.1) (2026-08-08)

### Continuous Integration

* add ICU caching and improve release workflow ([#24](https://github.com/harubi/bolivar/issues/24)) ([50c7b01](https://github.com/harubi/bolivar/commit/50c7b01cdbf960aecbe965ccd23a2c4ac1dd16b0))

## [1.9.0](https://github.com/harubi/bolivar/compare/v1.8.0...v1.9.0) (2026-08-08)

### Features

* add raw document extraction and metadata API ([3a7513b](https://github.com/harubi/bolivar/commit/3a7513b7d176ebc6e3bfbbd7b305a2b64619d666))
* **bidi:** add opt in text reconstruction ([724a221](https://github.com/harubi/bolivar/commit/724a221021ca5f59af317588702bc77870fb070a))

### Bug Fixes

* **bidi:** preserve legacy output ([f2f152a](https://github.com/harubi/bolivar/commit/f2f152a544e014027655e6ebdc6bca0be016c64b))
* **core:** preserve bidi extraction order ([90c12f0](https://github.com/harubi/bolivar/commit/90c12f003eff3d672ffe7c9aa9476fa2aeff1f47))
* **python:** declare document permissions ([26bf6b2](https://github.com/harubi/bolivar/commit/26bf6b2c2ba4dac498fd961cc611fc1f5b441171))
* **release:** retry failed publish ([#23](https://github.com/harubi/bolivar/issues/23)) ([0ea381f](https://github.com/harubi/bolivar/commit/0ea381fe2c82e29d65ae51da19da1e10cade2963))

### Performance Improvements

* **core:** reduce extraction overhead ([30837bb](https://github.com/harubi/bolivar/commit/30837bb1a8ea81c9eb28e38793bd8de263db9cb1))
* **jvm:** reduce binding overhead ([57b1cd6](https://github.com/harubi/bolivar/commit/57b1cd614a482f31cba26bff0aa89e1c039cc8ed))
* **uniffi:** reduce extraction copies ([f7b8fd4](https://github.com/harubi/bolivar/commit/f7b8fd4115a0b0f2e302a56eac20e0e2f713d275))

### Miscellaneous Chores

* **release:** 1.9.0 [skip ci] ([29e47bd](https://github.com/harubi/bolivar/commit/29e47bd8ecca1a0e7d45a7cf9f8a2e851f04b5bc))

### Code Refactoring

* **python:** tighten compatibility types ([2453015](https://github.com/harubi/bolivar/commit/2453015e333b5b1bdd12a885e28658e336085b79))

### Build System

* **icu:** add internal static crate ([afe5cff](https://github.com/harubi/bolivar/commit/afe5cff3fda21f17b98515e0ea3dcabec561f6cc))
* **python:** include test dependencies ([1abb961](https://github.com/harubi/bolivar/commit/1abb961fe1dd9ec6ec0ba9161c5df0096289d9fd))

### Continuous Integration

* support ICU builds ([b94386f](https://github.com/harubi/bolivar/commit/b94386f52d1e5183e59bf06d974ad9fe956b2ad6))
* verify static ICU builds ([c25d849](https://github.com/harubi/bolivar/commit/c25d849074e69d2a55fb8b716834e5323258860c))

## [1.8.0](https://github.com/harubi/bolivar/compare/v1.7.0...v1.8.0) (2026-07-19)

### Features

* add table extraction with options ([7edd5a3](https://github.com/harubi/bolivar/commit/7edd5a37399938bb8177be40b3cc31cf530bbad3))

## [1.7.0](https://github.com/harubi/bolivar/compare/v1.6.1...v1.7.0) (2026-05-22)

### Features

* **jvm:** add clojure bindings ([af4d2ad](https://github.com/harubi/bolivar/commit/af4d2ad3773dc8910bf002b1a0dabb61bae8cc59))

### Bug Fixes

* **cli:** preserve rotation in extract options ([4b18f37](https://github.com/harubi/bolivar/commit/4b18f375ee82ef900f27cf957b49c5e2b76b1990))
* **core:** harden compatibility and extraction contracts ([4199f9f](https://github.com/harubi/bolivar/commit/4199f9f11d50d0a4bd6c72b3135262ba4252eb39))
* **core:** normalize positive embedded font descent ([12f07b1](https://github.com/harubi/bolivar/commit/12f07b1ca36285595bbe6dc379b292ed392fdca3))
* **pdfminer:** accept codec in high_level.extract_text ([af8dee8](https://github.com/harubi/bolivar/commit/af8dee8e38d3a2ee0bf62568f5ca885b33b155ce))
* **pdfminer:** align fallback error edge cases ([3aceb24](https://github.com/harubi/bolivar/commit/3aceb24946e15429553faf8c11c1f81983908955))
* **pdfminer:** expose render_contents compatibility seam ([b0e198c](https://github.com/harubi/bolivar/commit/b0e198ca01b1cb4c7880462442a10e8cc7daef15))
* **pdfminer:** honor PDFDocument fallback contract ([5a93a34](https://github.com/harubi/bolivar/commit/5a93a3438315d2870f32a9e318a212c995767e4c))
* **pdfminer:** honor text-stream codec handling in extract_text_to_fp ([2335094](https://github.com/harubi/bolivar/commit/23350949282b9fbb8debd1e859fae3cfa46fd939))
* **pdfminer:** keep LTPage iteration to direct children ([7a817a2](https://github.com/harubi/bolivar/commit/7a817a2f76dfd8247a9bbd1342dabe81d945757c))
* **pdfminer:** match upstream text output in extract_text_to_fp ([ee07e8e](https://github.com/harubi/bolivar/commit/ee07e8e42225c4929826c37e246efdb713db6e8a))
* **pdfminer:** match upstream text-stream headers ([b7dd57b](https://github.com/harubi/bolivar/commit/b7dd57be5420b3f6d7c2b08acee3b71391202623))
* **pdfminer:** restore interpreter stack helpers ([0c1a919](https://github.com/harubi/bolivar/commit/0c1a9195df188af39627ba264559ed7aa755b0b7))
* **pdfminer:** restore tag and rotation parity ([f4e2d66](https://github.com/harubi/bolivar/commit/f4e2d66a144fd38df8307d5b18e90f95bbbf776a))
* **pdfminer:** support plain PDFDevice subclasses ([df73189](https://github.com/harubi/bolivar/commit/df73189705bef0174a10a12f4b738d76fd10bdc0))
* **pdfminer:** treat empty page selections like upstream ([6e94d74](https://github.com/harubi/bolivar/commit/6e94d74b997a50654e494345ae3a0f7a36a083a7))
* **pdfplumber:** make patched pdf.pages list-compatible ([f2da92f](https://github.com/harubi/bolivar/commit/f2da92f270bf894c48218f961dd8114b2c975922))
* **pdfplumber:** normalize grayscale non-stroking colors ([ff68303](https://github.com/harubi/bolivar/commit/ff683034c1ba0804cae8701222a8beb13a90f035))
* **pdfplumber:** restore base14 font descent geometry ([29799a6](https://github.com/harubi/bolivar/commit/29799a6ec25b85e951377aa84f9180a2b9cb6097))
* **pdfplumber:** restore extract_text textmap semantics ([538ba59](https://github.com/harubi/bolivar/commit/538ba59eee486a4e45d675d270db4bf6da4f6855))
* **pdfplumber:** suggest close table setting names ([19753e3](https://github.com/harubi/bolivar/commit/19753e311bb2aab336db78fb3718a072a169b334))
* **python:** expose lazy exports through module dir ([4e3b708](https://github.com/harubi/bolivar/commit/4e3b708782c803625d703b7e8222936990d271c0))
* **python:** make stub parity checker understand manifest-backed exports ([0da69d6](https://github.com/harubi/bolivar/commit/0da69d6a29393ddcd41e7b742963bdad087e2c9a))
* **python:** tighten shim boundaries and compatibility contracts ([6874e89](https://github.com/harubi/bolivar/commit/6874e89abfb40203f5d902f4602b086c5f56c59a))
* **runtime:** harden single-pipeline extraction contracts ([dde6f91](https://github.com/harubi/bolivar/commit/dde6f91cfbc4fd3ff4e041e058f68ae32299e5f7))
* **test:** point font_size_test at fixtures ([b7feea1](https://github.com/harubi/bolivar/commit/b7feea1c5a2ce23cd89f6592b841798f7e288e49))

### Performance Improvements

* **core:** remove serial page warmup ([9c078a8](https://github.com/harubi/bolivar/commit/9c078a8f92737ed86fb4a9780ff153878190bec8))
* **table:** avoid cloning edge sets when line filtering ([f065f95](https://github.com/harubi/bolivar/commit/f065f952616f56973c552043f26e3bdfb47a3cb5))

### Miscellaneous Chores

* **core:** final sweep ([d9c2db3](https://github.com/harubi/bolivar/commit/d9c2db38f67b04d4a99c751fdbe21cf3a3305f41))
* **gitignore:** dedupe rules and add patterns ([2d6c385](https://github.com/harubi/bolivar/commit/2d6c38556e4df1f5c46ef03e875233c06aaf4f4e))
* remove tracked cruft and gradle artifacts ([abe043d](https://github.com/harubi/bolivar/commit/abe043d1a817c279d08dafdbfafa915e2f37555b))
* simplify build tasks ([a940be5](https://github.com/harubi/bolivar/commit/a940be59192e1d8ea17188f659e0fb19ddea46cd))

### Code Refactoring

* **bridge:** remove dead table stream export ([115f5da](https://github.com/harubi/bolivar/commit/115f5da7b635456158ba7f8fb7aa943074a31218))
* **core:** clean dead code and stale TODOs ([e5391bc](https://github.com/harubi/bolivar/commit/e5391bc2f7111f33d78599dc98b506a45cb776e6))
* **core:** collapse Page/TableStream into generic Stream<R> ([1a84b9c](https://github.com/harubi/bolivar/commit/1a84b9c5218aaffa67b2f6cd32802bda6b748a1f))
* **core:** delete redundant collect-flavored extraction APIs ([14b798b](https://github.com/harubi/bolivar/commit/14b798be95b6c2acd4b13454d7fbbd8aa129ebf1))
* **core:** extract finisher helpers and clarify process_page doc ([a6617b9](https://github.com/harubi/bolivar/commit/a6617b9d06fb2a5f59ef9e6bf1c1d308852d22f9))
* **core:** extract PDFDevice trait into device/ module ([1a40c30](https://github.com/harubi/bolivar/commit/1a40c306470d61b75ac0ebb20daf8945af942efc))
* **core:** introduce run_batch and migrate par_iter callers ([b36bf2f](https://github.com/harubi/bolivar/commit/b36bf2fd6a58295db81100cfadbae5bd43230ac9))
* **core:** merge soa_layout into soa ([00f88bf](https://github.com/harubi/bolivar/commit/00f88bfb0c4006afafbb6a05c40bcc8445c95d9d))
* **core:** parallelize image extraction ([1284d6f](https://github.com/harubi/bolivar/commit/1284d6f82487e666a9b0126822470d07f039eeda))
* **core:** prune unused lib aliases ([3ba59c9](https://github.com/harubi/bolivar/commit/3ba59c924d06a0f5b088ab49ef71ca311712ce82))
* **core:** rename converter/ to device/ and split base.rs ([b6c998d](https://github.com/harubi/bolivar/commit/b6c998d5753537e8323a5542726fea1e80ee2e11))
* **core:** split api into engine and extract ([0e190db](https://github.com/harubi/bolivar/commit/0e190dbef8a59de868125dbb3132f1aa426600a6))
* **core:** unify extraction around a single document pipeline ([ce47968](https://github.com/harubi/bolivar/commit/ce479680f8a950d94a56f5db891dd5bcf92f2596))
* **core:** unify process_page across devices ([4c658e5](https://github.com/harubi/bolivar/commit/4c658e5644d8ba42ef59af4798f349aea0ed1434))
* **core:** use impl suffix on extract_text internals ([20e9ec4](https://github.com/harubi/bolivar/commit/20e9ec4f93f23c8bc964340e6d4f3e1eee6894c6))
* **pdfminer:** align high_level codec with upstream signature ([6b146bd](https://github.com/harubi/bolivar/commit/6b146bd4a2fb24d6d4f370ff068105f758dbc29c))
* **pdfplumber:** isolate compat-only table helper ([c4ceaaf](https://github.com/harubi/bolivar/commit/c4ceaafa9a72d426ede6607bcd0189a1e5d26630))
* **pdfplumber:** narrow compat table geometry contract ([4c2f6f2](https://github.com/harubi/bolivar/commit/4c2f6f2f7710c190fd6d909465a541c407894080))
* **pdfplumber:** remove dead lazy page membership cache ([fccf8e6](https://github.com/harubi/bolivar/commit/fccf8e60f8f173c561f3d1b04db743793c315415))
* **pdfplumber:** remove dead table geometry cache ([f7601a4](https://github.com/harubi/bolivar/commit/f7601a455f035a94d30685c7e1af948d5343f04a))
* **pdfplumber:** replace single-page words stream bridge ([db80a2a](https://github.com/harubi/bolivar/commit/db80a2ac9df9be3631f51c023f664830cd4ae6f5))
* **pdfplumber:** require explicit compat table inputs ([4561128](https://github.com/harubi/bolivar/commit/456112845aadf27ca9d0ecf0e04464111a550cf9))
* **pdfplumber:** require indexed backend for original pages ([dc1349d](https://github.com/harubi/bolivar/commit/dc1349d3a9281c00c8dcfa315a445c6bd1facc73))
* **pdfplumber:** trim compat page protocol ([ae48907](https://github.com/harubi/bolivar/commit/ae48907a11a5e66d3e2c074c48da2178eba8906b))
* **python:** centralize native export manifests ([e3e8467](https://github.com/harubi/bolivar/commit/e3e8467e688963942711fe6c2cadbbbe5d30a1ac))
* **python:** collapse autoload bootstrap path ([73a65f4](https://github.com/harubi/bolivar/commit/73a65f4d37b743fe5182996730ac12c583b69921))
* **python:** collapse shim adapters and remove dead bridge code ([4848c45](https://github.com/harubi/bolivar/commit/4848c452826a61e2220dbbf9aead5e50d73b4dd6))
* **python:** declare composite stub symbol manifests ([a9f7d41](https://github.com/harubi/bolivar/commit/a9f7d418c39783a64d28e7f94ff44baeee69650e))
* **python:** hide lazy export internals from dir ([976ad70](https://github.com/harubi/bolivar/commit/976ad706bd6cee15b029284d98e76e70c17b8386))
* **python:** hide loader helpers from module attrs ([caab172](https://github.com/harubi/bolivar/commit/caab172f2d58ea9bbdc8cbbd81a12993420f5b6a))
* **python:** inline composite stub symbol manifest ([8acd2a6](https://github.com/harubi/bolivar/commit/8acd2a66fe332adbe68260340ec4a526ccb6561f))
* **python:** isolate compat table conversion helpers ([1120fbe](https://github.com/harubi/bolivar/commit/1120fbe32355155e2100399f398ccf7af11c0a8c))
* **python:** privatize native loader helpers ([d9d2aae](https://github.com/harubi/bolivar/commit/d9d2aaefe75317840d728dacdfeb04dde6691170))
* **python:** remove duplicate stub symbol manifest assignment ([ab7b7a3](https://github.com/harubi/bolivar/commit/ab7b7a3aa9247ef41641f335e71156b66cbb2863))
* **python:** remove preloaded autoload fallback ([e1b5dc3](https://github.com/harubi/bolivar/commit/e1b5dc35a7ba10e73c6cd4385584c941cbbf5fb7))
* **uniffi:** decompose lib.rs into per concern modules ([263baad](https://github.com/harubi/bolivar/commit/263baad029ed89829fe78fd360a805ac59ebbb83))
* **uniffi:** route tables through canonical metadata stream ([e3eb725](https://github.com/harubi/bolivar/commit/e3eb7250c8608a155c2d3e3b31df9b02ced299ad))

### Tests

* **core:** guard compat facade boundary ([af1f479](https://github.com/harubi/bolivar/commit/af1f479730faaaa097d2863313959886e4f24a39))
* **core:** repair high_level ExtractOptions defaults ([1cc9a36](https://github.com/harubi/bolivar/commit/1cc9a36cd1f6c140d3320681f0e1a0563911de57))
* gate skipped modules loud in CI ([427dd92](https://github.com/harubi/bolivar/commit/427dd9298a93cdc6d03d95535e91a2dd54c8358c))
* **parity:** lock single-pipeline regressions ([490d9e4](https://github.com/harubi/bolivar/commit/490d9e4afbf92be2524465e17828f6983912ffd1))
* **python:** guard bridge-only compat helper surface ([ba2f311](https://github.com/harubi/bolivar/commit/ba2f311bb4b3cfa1983282b8da6f5a415263aa2f))
* **python:** lock bolivar native surface boundary ([0066e91](https://github.com/harubi/bolivar/commit/0066e91754590aa66783737c8d3fcc0eb0c54248))
* **python:** lock compat table bridge surface ([c9c4209](https://github.com/harubi/bolivar/commit/c9c420938f8bd280fa86fb59cbb4e5bdff6d4503))
* **python:** lock native stub surface to export manifest ([ae534fb](https://github.com/harubi/bolivar/commit/ae534fb05904507bde603bc4ab7eea3663b66206))
* **python:** remove redundant dead bridge surface test ([a0f93a5](https://github.com/harubi/bolivar/commit/a0f93a5ce9660e2e2c9a799079e6e6f81906d12c))
* **python:** tighten bridge-only helper guards ([593248b](https://github.com/harubi/bolivar/commit/593248b386c5d551252087ab6cb37de108cd408f))
* **python:** verify bridge helper runtime surface ([79a5a90](https://github.com/harubi/bolivar/commit/79a5a90a55ca91e384e74be0b406c7e845f8e50a))
* **python:** verify loaded native extension exports against manifest ([0631f39](https://github.com/harubi/bolivar/commit/0631f390bcd104d451466a86b3e3d540edc44a9f))

### Continuous Integration

* drop autobuild for CodeQL rust extractor ([cf90a43](https://github.com/harubi/bolivar/commit/cf90a4319c3aabfb097a4a77d82ea9a664d6fed0))

## [1.6.1](https://github.com/harubi/bolivar/compare/v1.6.0...v1.6.1) (2026-03-05)

### Performance Improvements

* **pdfplumber:** make extract_tables use stream backend ([88a7956](https://github.com/harubi/bolivar/commit/88a7956bc5a1622d8fc51261da2df40866987e3f))

### Miscellaneous Chores

* **deps:** add pandas and pypdfium2 to typecheck group ([68893d6](https://github.com/harubi/bolivar/commit/68893d620ea7bd1a383ccfebc56ff3c08a8e8dc4))

## [1.6.0](https://github.com/harubi/bolivar/compare/v1.5.2...v1.6.0) (2026-03-04)

### Features

* **jvm:** add Kotlin facade and UniFFI bindings ([7fd7c25](https://github.com/harubi/bolivar/commit/7fd7c257433b09f6379bd40aa8013abce81cc9ac))

### Bug Fixes

* **core:** harden stream worker lifecycle on drop ([2591e8f](https://github.com/harubi/bolivar/commit/2591e8fb23ec3e9507ee0a4386692dbe875981d0))
* **python:** align bindings with PDFDict ([2d6a9ef](https://github.com/harubi/bolivar/commit/2d6a9ef843b5df199dfc2f1b1f8dc98762e2459f))
* **python:** enforce single-page table extraction contract ([e8fee22](https://github.com/harubi/bolivar/commit/e8fee228fb69542ec3a853ef884ced8b819e1c2d))
* **python:** match pdfplumber parity for normalization and text flow ([17949e4](https://github.com/harubi/bolivar/commit/17949e498d6012e553bff7b90a124a0acb332c56))
* **uniffi:** use PDFDict in page geometry tests ([d228b08](https://github.com/harubi/bolivar/commit/d228b08cb609330d89bd511f06d5212a6db4c837))

### Performance Improvements

* **core:** reduce PDF object allocation ([b410af2](https://github.com/harubi/bolivar/commit/b410af274a02984ee7ae5c2b8eb771c4d67a4952))

### Code Refactoring

* **core:** align dict/name types across core and tests ([9cc25b5](https://github.com/harubi/bolivar/commit/9cc25b51d4c013a09c8e63aa261807e7e8216b60))

### Tests

* **parity:** harden parity runner and fixture-dependent tests ([6db12b7](https://github.com/harubi/bolivar/commit/6db12b7bfbecb58d6adc01b5960b4a933dbb6ebe))

## [1.5.2](https://github.com/harubi/bolivar/compare/v1.5.1...v1.5.2) (2026-02-22)

### Performance Improvements

* **python:** offload more api to rust ([cb00bc1](https://github.com/harubi/bolivar/commit/cb00bc1a0dbc4bc1bb97fc6af86d9c59c94eb671))
* **python:** offload pdfplumber text/words stream ([33a2775](https://github.com/harubi/bolivar/commit/33a2775fd2e1bd9d81c0bcee511ee9cab71828af))

### Continuous Integration

* trim release matrices ([a368d0a](https://github.com/harubi/bolivar/commit/a368d0a82e355e7b66de508c06919db1fa5acb45))

## [1.5.1](https://github.com/harubi/bolivar/compare/v1.5.0...v1.5.1) (2026-02-22)

### Performance Improvements

* **python:** lazily materialize page attrs ([7b29aee](https://github.com/harubi/bolivar/commit/7b29aee79dfa5ae195f69b9d1f7a2027b1f9cc95))

### Miscellaneous Chores

* **jvm:** update fqn ([96e0674](https://github.com/harubi/bolivar/commit/96e067497e1ca1bd161fb97002561462a97d3b68))

## [1.5.0](https://github.com/harubi/bolivar/compare/v1.4.0...v1.5.0) (2026-02-14)

### Features

* **python:** increase type coverage ([c5b20d1](https://github.com/harubi/bolivar/commit/c5b20d11545ee44c03a323380a298e9090e5a90b))

## [1.4.0](https://github.com/harubi/bolivar/compare/v1.3.1...v1.4.0) (2026-02-12)

### Features

* **python:** add types to python apis ([b22f66f](https://github.com/harubi/bolivar/commit/b22f66fd33be4b49d18e47534137ab039dd5b3a1))

### Performance Improvements

* **ci:** enhance CI build speed ([ab4d0bb](https://github.com/harubi/bolivar/commit/ab4d0bb98a5a3f96a3e6863a4819eb686f42a80b))

## [1.3.1](https://github.com/harubi/bolivar/compare/v1.3.0...v1.3.1) (2026-02-12)

### Bug Fixes

* **ci:** pin nightly toolchain and unify cache ([7ad69dc](https://github.com/harubi/bolivar/commit/7ad69dc66ec0e904d8dd0b25e419ebc4eaef742b))
* **release:** make semantic-release trigger on all ([7ed4ec1](https://github.com/harubi/bolivar/commit/7ed4ec1107c67d2c177b62b287626cc73b37ed85))

### Code Refactoring

* **python:** make api strictly-typed ([21fafa2](https://github.com/harubi/bolivar/commit/21fafa22e025421525c96b8cf076d2cae2c97d2b))

## [1.3.0](https://github.com/harubi/bolivar/compare/v1.2.0...v1.3.0) (2026-02-10)

### Features

* **uniffi:** redesign kotlin api ([6232220](https://github.com/harubi/bolivar/commit/6232220eec49d88aefba07ec8e9eaad8143d0d09))

### Bug Fixes

* enable cache in workflows ([11b3d4a](https://github.com/harubi/bolivar/commit/11b3d4a5e997159d23b4d3c2b4a82adbe083227a))
* update readme ([1c95580](https://github.com/harubi/bolivar/commit/1c9558088431a27b84de8ca1117190ea7251738e))

## [1.2.0](https://github.com/harubi/bolivar/compare/v1.1.0...v1.2.0) (2026-02-10)

### Features

* **release:** restructure gh actions ([4289ef7](https://github.com/harubi/bolivar/commit/4289ef7320ced956e273fe8bfc542cd70905dbcd))

## [1.1.0](https://github.com/harubi/bolivar/compare/v1.0.0...v1.1.0) (2026-02-10)

### Features

* **release:** use downloaded native lib for uniffi ([ecbcc4a](https://github.com/harubi/bolivar/commit/ecbcc4ae34ad0073cad4c90718398ce3f8dde407))

## 1.0.0 (2026-02-10)

### Features

* add `lt` tree ([dc3547f](https://github.com/harubi/bolivar/commit/dc3547f7566e6f60999d4464dd90bd2af81fad42))
* add `matrix`/`mcid` ([349b4e1](https://github.com/harubi/bolivar/commit/349b4e187d2aea96ad991dcf6815aaa69e8b499d))
* add `xrefs` accessors ([2cb480a](https://github.com/harubi/bolivar/commit/2cb480a1db3dae5289e2d7da666f3a4f0bb6e570))
* add cargo makefile ([6f45343](https://github.com/harubi/bolivar/commit/6f45343d5b5f1f0b15d5a6263afef3fef96bde68))
* add shim registry ([092f1e9](https://github.com/harubi/bolivar/commit/092f1e9af116ddc4f4d6d9f765e916c8d161411d))
* **api:** add ordered extract_pages_stream ([4ba7ab1](https://github.com/harubi/bolivar/commit/4ba7ab155b9ccf6b071d292d6cff7906fc02e48b))
* **arena:** add arena image/figure materialization ([f01f45d](https://github.com/harubi/bolivar/commit/f01f45dc97d8e151940a58568c8bbbbecd21d259))
* **arena:** add color pooling and shape items ([d949657](https://github.com/harubi/bolivar/commit/d949657f205403b953603433416bd76b8d4ab315))
* **build:** add Makefile.toml ([b9e08f0](https://github.com/harubi/bolivar/commit/b9e08f0feb9284ca856f1f6843373cec9e5af085))
* byte-safe PDF names ([865d300](https://github.com/harubi/bolivar/commit/865d30025beb1fc24533017bd848e318ed81aa42))
* **cli:** add table extraction ([29b2381](https://github.com/harubi/bolivar/commit/29b2381d8307fc9095cc748ce89ae931dbd125f5))
* **cli:** add table settings overrides ([ce98c2d](https://github.com/harubi/bolivar/commit/ce98c2d813786a4cf51f94bd52f5ac2f2ef09f30))
* **cli:** stream table output per page ([d3b18a1](https://github.com/harubi/bolivar/commit/d3b18a11ffd11563db54489ace2c9d763ae5670b))
* **cli:** use mmap ([070e9a9](https://github.com/harubi/bolivar/commit/070e9a9ce81e4c5dfe3c6517eadb2fca154dca1d))
* **converter:** add hOCR output ([bad3227](https://github.com/harubi/bolivar/commit/bad32279d2b3274601a3613afdaa75ce95566e7d))
* **converter:** add rtl text reordering for html conversion ([735b230](https://github.com/harubi/bolivar/commit/735b230fd9d84a031b73782155ed89a565cbe274))
* **core:** add `ObjStm` fallback ([2ed47cf](https://github.com/harubi/bolivar/commit/2ed47cfbc14eb3a59509735b2ed7566de44635cd))
* **core:** add layout cache and thread normalization ([94ae6b4](https://github.com/harubi/bolivar/commit/94ae6b409f706b0e8d328c4153cea389435589ec))
* **core:** add LRU caching and lazy access ([0bf48ae](https://github.com/harubi/bolivar/commit/0bf48aeebef0e96259b46f68cd27acbbc25d809b))
* **core:** add marked content and color space ([a28d113](https://github.com/harubi/bolivar/commit/a28d11333cfa85eb846996c67f1bb512e08a7abc))
* **core:** add parallel page processing with rayon ([8b9b304](https://github.com/harubi/bolivar/commit/8b9b304357ad37ec6ac8e2f392faaf5f2bf02507))
* **core:** add stream extraction for PDFDocument ([9c48687](https://github.com/harubi/bolivar/commit/9c48687d6ef4eb1f051b6b2836f048bdd0b376f2))
* **core:** add table and text extraction with new font handling ([c966f44](https://github.com/harubi/bolivar/commit/c966f44afd0ba56b7f587e5bc9c20baa7fed470c))
* **core:** add utility methods ([1a2b5f4](https://github.com/harubi/bolivar/commit/1a2b5f4d3000bf04164424407d5c4da82ad4bd0b))
* **core:** apply bidi to HOCR output ([a980aa0](https://github.com/harubi/bolivar/commit/a980aa04b0bb17b347d5b9555396f38da0f17bf9))
* **core:** cache page index for fast page access ([a1c0922](https://github.com/harubi/bolivar/commit/a1c0922801d07e8c2bb9aec182f62a0c590af1ca))
* **core:** reorder RTL text in table extraction ([6f58de2](https://github.com/harubi/bolivar/commit/6f58de2c66b05fd3dc23a1b7fc3376525646d9a2))
* **core:** reorder RTL text in XML output ([fa96193](https://github.com/harubi/bolivar/commit/fa96193c4629efeec53f11fe0f4c4ece4a1813ca))
* **document:** add shared resolve cache ([4d9397e](https://github.com/harubi/bolivar/commit/4d9397ecedc75bbf825b7a521e127f3d6eecace0))
* **document:** store document as `Bytes` ([27f220e](https://github.com/harubi/bolivar/commit/27f220ef39225c189e1e81313673fa23f12dea93))
* **document:** zero-copy decode fast path ([fc3d597](https://github.com/harubi/bolivar/commit/fc3d597c3b5c73ef8b88f4ff1a8b4e35427a26c2))
* enforce bolivar shims ([c4852d6](https://github.com/harubi/bolivar/commit/c4852d68c07215f4cf5ab54b417434fca0006820))
* **examples:** add analysis cli for tuning KNN ([4cb221b](https://github.com/harubi/bolivar/commit/4cb221b36504525203d3e32fa48aa5315e992fce))
* **interp:** parse content streams without concatenation ([19add69](https://github.com/harubi/bolivar/commit/19add69c0622a64e5af3b712a330e7614328244c))
* **layout:** add color for operations ([45821ff](https://github.com/harubi/bolivar/commit/45821ffaf2daab57b084f0da117f223d4ac84f18))
* **layout:** add dynamic spatial trees ([0d6707a](https://github.com/harubi/bolivar/commit/0d6707a16ac3fdf7ffcf0538df8e539c27b14f04))
* **layout:** add LayoutSoA container ([a3711a5](https://github.com/harubi/bolivar/commit/a3711a5d7da7f327c03de179cc7ecb2550d16ce8))
* **layout:** add MCID for tagged PDF ([2452dc0](https://github.com/harubi/bolivar/commit/2452dc0ca1ca892f716cf14123a75b5b83ada48c))
* **layout:** add pdfminer grouping methods ([022eca1](https://github.com/harubi/bolivar/commit/022eca153648f6dcb6a3dd03d68881241098f782))
* **layout:** preserve `Form XObject` ([ffcb519](https://github.com/harubi/bolivar/commit/ffcb519f45d389323dd46b8d60213bfb55abf3b3))
* **layout:** track leaf membership ([e6915ba](https://github.com/harubi/bolivar/commit/e6915ba635553918d611acc011e0464ed009c241))
* **parser:** add specialized content lexer with simd ([812c5f3](https://github.com/harubi/bolivar/commit/812c5f3130449d5d2daca44232b61ff8dffd1f0a))
* **pdfinterp:** render Form XObjects ([056166e](https://github.com/harubi/bolivar/commit/056166e802c26565562d1f3038be2aa8690680c8))
* **pdfminer:** add the last optional apis for parity ([d912e70](https://github.com/harubi/bolivar/commit/d912e706ce47be94f767be353c93523654e5556f))
* **pdfpage:** use inherited attrs chain for page traversa ([53f7d6a](https://github.com/harubi/bolivar/commit/53f7d6a666627f0ebd43daa472e43455ea38acb5))
* **pdfstate:** add `fontname` ([5cae84a](https://github.com/harubi/bolivar/commit/5cae84a07efb570a2f5b465f94ac61bbfe54bd2b))
* **plane:** add k-nearest neighbors query method ([36ceb47](https://github.com/harubi/bolivar/commit/36ceb47b143ac52696ab214381c18705d4dce533))
* **psparser:** zero-alloc in postscript parser ([d3bf2c0](https://github.com/harubi/bolivar/commit/d3bf2c0f75ac2d722c7791a0af63e779d99cfc14))
* **python:** add `extract_table_from_page` ([1552e1e](https://github.com/harubi/bolivar/commit/1552e1ebc976941c10c2b2722a34ca6230d4899b))
* **python:** add `PDFObjRef` resolve ([a73a903](https://github.com/harubi/bolivar/commit/a73a90338182870be5b39735a08fe0e5d20dee5b))
* **python:** add async page stream and pyo3 runtime poc ([eca6b0b](https://github.com/harubi/bolivar/commit/eca6b0b076f383ecfc5741771e6e7254b2c75789))
* **python:** add layout cache for pdfplumber tables ([65c2569](https://github.com/harubi/bolivar/commit/65c256999421f3ff0d576d838cf413f49a737045))
* **python:** add lazy page access ([62de233](https://github.com/harubi/bolivar/commit/62de2335b5149e1959c0314f1f8b135a1ae9b42d))
* **python:** add most of the pdfminer shim ([0e36bfc](https://github.com/harubi/bolivar/commit/0e36bfc837d250661a19eb385b308878049a6547))
* **python:** add pdfminer codec shims ([d39478a](https://github.com/harubi/bolivar/commit/d39478a1703a12ce823a29dd2cffcf3830ae0ea7))
* **python:** add pdfminer compatibility shim with stubs ([bb8d1da](https://github.com/harubi/bolivar/commit/bb8d1daae65114e8563854baf39540d46a6203c7))
* **python:** caching param and reuse `LTPage` tables ([ab90423](https://github.com/harubi/bolivar/commit/ab9042345493c9896d481a1ee22f1dbbc4da015c))
* **python:** expose cmap/encoding/font APIs ([6a57a7b](https://github.com/harubi/bolivar/commit/6a57a7bfe65076b518f2811ed9ac30181f12d015))
* **python:** expose codec APIs ([cf7c842](https://github.com/harubi/bolivar/commit/cf7c84241e60288e8b5da0ab4af2bfcc29f0af2f))
* **python:** full pdfminer/pdfplumber parity ([0e11e21](https://github.com/harubi/bolivar/commit/0e11e214b29324021072330ba58aa99004f451df))
* **python:** fully offload table extraction (again) ([46de42a](https://github.com/harubi/bolivar/commit/46de42a62a10d49be2d2a13a9f04a306788d886f))
* **python:** offload most the heavy lifting to rust ([51fb414](https://github.com/harubi/bolivar/commit/51fb414175e46b54243f14498408e3f23ec0910d))
* **python:** patch pdfplumber `extract_table(s)` ([42348a9](https://github.com/harubi/bolivar/commit/42348a9ed62b3b4eb0fbdfc1019f2acf5f7aa5ed))
* **python:** stream pages async from PDFDocument ([bea2fc7](https://github.com/harubi/bolivar/commit/bea2fc75b8dcceb5ccedaa2d4f729d09e8e1072e))
* **python:** zero-copy python input ([139542b](https://github.com/harubi/bolivar/commit/139542bba0ba3fbb4261442d224f825dd1d6425b))
* **table:** add `extract_table_from_ltpage` ([e732965](https://github.com/harubi/bolivar/commit/e732965e1611b68a32cf9ec16eb1989a471e36c0))
* **table:** better bidirectional text reordering ([ffa7e98](https://github.com/harubi/bolivar/commit/ffa7e98078045b4cb970e83125827b8d8ce88895))
* **tests:** add page aggregator materialization test ([0df17c0](https://github.com/harubi/bolivar/commit/0df17c0b49e3cd71598e22bdabdd2d0477caebfe))
* **text:** add bidi support ([4f233dd](https://github.com/harubi/bolivar/commit/4f233ddd67205e033c918d67d492a83a8be53753))
* **uniffi:** add Kotlin/JVM bindings with async offload ([9e44d6d](https://github.com/harubi/bolivar/commit/9e44d6d6cea765fedef2a269e68a600a1cadf21c))
* use geo indexes ([2c8db8a](https://github.com/harubi/bolivar/commit/2c8db8ac283e17fb3dfe4636084b6da9f766ab0f))
* **workspace:** split project into workspaces ([f3e82a6](https://github.com/harubi/bolivar/commit/f3e82a66d03436436f0e0707d3a15931ea084fe3))
* wrap `lt` items ([09c441b](https://github.com/harubi/bolivar/commit/09c441b754e41c6d44564d4c45454337499da61f))
* zero-copy stream slices ([3daccbe](https://github.com/harubi/bolivar/commit/3daccbe9836849e6e29122bb01495a6f2fd53b4f))

### Bug Fixes

* bound pdfplumber table stream cache ([1c702bf](https://github.com/harubi/bolivar/commit/1c702bfee6496fb8ec24555a5e0f5419fbd5f97e))
* **ci:** init submodules in container tests ([7b50407](https://github.com/harubi/bolivar/commit/7b504071ba366c1e15d7732031c519cf48fadc67))
* **ci:** remove redundant `cargo publish` flag ([a69dcb0](https://github.com/harubi/bolivar/commit/a69dcb05a15bb440289002a9efcf765432df0db4))
* **ci:** remove redundant `cargo publish` flag ([018bbd2](https://github.com/harubi/bolivar/commit/018bbd28ed17c4e2d4efe3147ae995ead529f297))
* **cli:** set default thread count ([ff69ed9](https://github.com/harubi/bolivar/commit/ff69ed9ce16940198437a13d2570b90d69955002))
* **cmapdb:** split on `\\r` ([7d73096](https://github.com/harubi/bolivar/commit/7d73096a73a3228b48a80c88456c3b7ef72906eb))
* **cmap:** parse bfchar pairs ([5b6fbbe](https://github.com/harubi/bolivar/commit/5b6fbbeed4b42f479531cdc899c2e995fb1df365))
* **converter:** use real fontname ([215aa5c](https://github.com/harubi/bolivar/commit/215aa5c4ec2ec56d588afd338942b791e9d7d6b9))
* **core:** avoid double bidi in table text extraction ([8e95162](https://github.com/harubi/bolivar/commit/8e9516267adaefa54f7c49ed53e57b2b7a41755a))
* **core:** detect circular ref in object resolution ([3d48de1](https://github.com/harubi/bolivar/commit/3d48de1525522d437445873b0288c2922b4e882d))
* **core:** normalize table edge bboxes ([b6fe7cc](https://github.com/harubi/bolivar/commit/b6fe7cc3b38db4574e2aabec6eefbeb8473f0950))
* **core:** scope page counter to document ([8e4dab0](https://github.com/harubi/bolivar/commit/8e4dab0be383ce34398a6cc916f621a203d4ac8c))
* **core:** use thread-local to avoid circular ref ([a0b09bf](https://github.com/harubi/bolivar/commit/a0b09bf2f718d4420b1f371c19445a80f4b9f60f))
* flatten `LTPage` iter ([1d86d4c](https://github.com/harubi/bolivar/commit/1d86d4c726356756afb521ad5e59d0bd52613d8f))
* guard `xobjects` ([c0efce4](https://github.com/harubi/bolivar/commit/c0efce476487b3fa2d93f6c0f5954700f268ccc1))
* **high_level:** default laparams ([616590b](https://github.com/harubi/bolivar/commit/616590b6bef7ba25c6ba691cb0569aaaacd28d15))
* **page:** require `MediaBox` ([48684ff](https://github.com/harubi/bolivar/commit/48684ff21d596492e2a2c3722ca8ab69eeca1da5))
* **parser:** guard `pos` ([e2eb907](https://github.com/harubi/bolivar/commit/e2eb907cbc37e2ef9677117597375cad41a2a193))
* **parser:** handle unknown in object parsing ([3b8eb27](https://github.com/harubi/bolivar/commit/3b8eb2729c9de6c6730d0474997ae92d91965aa1))
* **pdffont:** use `cid2unicode` ([435d983](https://github.com/harubi/bolivar/commit/435d9832e1651b7dd8d078eb4e0c8e52521a3b39))
* **pdfinterp:** set textstate fontname ([5a5b297](https://github.com/harubi/bolivar/commit/5a5b297cfebb7ba4bb4ed3a3bbe57e4ca00dfda1))
* **pdfplumber:** avoid caching pages during async iteration ([ba46bc4](https://github.com/harubi/bolivar/commit/ba46bc4883df4e0241a89be3aaa7f8dc5a3f3e2f))
* **pdfplumber:** avoid document-wide table extraction ([1de3eb0](https://github.com/harubi/bolivar/commit/1de3eb01c3d6b4665842fc85ae392bdc60c7185c))
* **pdfplumber:** make async pages lazy ([19b937f](https://github.com/harubi/bolivar/commit/19b937f2d3f07ced7fb3fce397517519b8199c5c))
* **python:** accept list bbox inputs ([dd21b78](https://github.com/harubi/bolivar/commit/dd21b78d775f2f4d7900146e81137c4d425c5980))
* **python:** add layout accessors and text ([ca43806](https://github.com/harubi/bolivar/commit/ca43806b21e08f039f8ec04430d0070b8fabe19b))
* **python:** default laparams ([8834359](https://github.com/harubi/bolivar/commit/8834359f9e931bc8d29ee30e19c3bd08f6c34bed))
* **python:** expose LTChar colorspace ([da5ec45](https://github.com/harubi/bolivar/commit/da5ec45d3fc4bc699f8c51ffefc01a5c910ebbf1))
* **python:** fix autoload import path ([62d5ba9](https://github.com/harubi/bolivar/commit/62d5ba9b24d5fe4b4ae2171af54bf6a9cfdbabb9))
* **python:** flatten LTPage iteration ([47d3353](https://github.com/harubi/bolivar/commit/47d33537d645cfa0c76771c57263a6e932e02d59))
* **python:** guard `pages` ([fed7da5](https://github.com/harubi/bolivar/commit/fed7da5666bbe7bfd5f72a8997be0f8c787a8936))
* **python:** guard infinite recursion in `NumberTree` ([6e86a74](https://github.com/harubi/bolivar/commit/6e86a74af7bfa86c40a1af6e71800c3e1777abe7))
* **python:** handle empty path segments ([19ad058](https://github.com/harubi/bolivar/commit/19ad0586542ad8b21a70d22740f7c4b559a1eeb9))
* **python:** handle nested `PDFObjRef` in `resolve1` ([8f014ab](https://github.com/harubi/bolivar/commit/8f014abacd9fb7713c7fd659901f5635f6e7c01f))
* **python:** preserve `PDFObjRef` in page attrs ([dd9d27f](https://github.com/harubi/bolivar/commit/dd9d27f5c9eef9bd6c2b17b52be14f2b0d86f404))
* **python:** remove unsendable parser drop panic ([a65c049](https://github.com/harubi/bolivar/commit/a65c0496f91822cf5681b9a7a4f5cdfc1410a979))
* **python:** resolve detach result type ([5992455](https://github.com/harubi/bolivar/commit/59924558ca986fa3c3db981a03d2681086acd685))
* **python:** resolve page attrs ([feeeee3](https://github.com/harubi/bolivar/commit/feeeee39e4f2669796b9e81717d588558612677b))
* **python:** restore closure hook ([b2e68f7](https://github.com/harubi/bolivar/commit/b2e68f71e733decc165c2f83fe4a5ca8280821e3))
* **python:** restore filtered table extraction ([7f7e9b7](https://github.com/harubi/bolivar/commit/7f7e9b76f9da48a09e7c03b4343753b040499fbd))
* **python:** restore lazy layout wrapper ([6221b9a](https://github.com/harubi/bolivar/commit/6221b9a879eea6055dd83b5008bc21a6997e6d36))
* **python:** restore pdfminer layout compat ([2bf529b](https://github.com/harubi/bolivar/commit/2bf529bc5fbf6dc07d1a929e4f04110dd9b1b026))
* **python:** update pyo3 casts ([3d1d66b](https://github.com/harubi/bolivar/commit/3d1d66bc2c947982bce1fa4c247829e4073966e8))
* **release:** adjust release configs and scripts ([94d9aee](https://github.com/harubi/bolivar/commit/94d9aeebecbf12e6325a6f7ec9e157de3708a917))
* **release:** reset releases for adjusted actions ([ed683cc](https://github.com/harubi/bolivar/commit/ed683ccb8c29da511b485802f7097a0dc47528f8))
* **rtl:** fix e2e rtl extraction ([508177a](https://github.com/harubi/bolivar/commit/508177a1a668ec39293a45236a6c41dd01181156))
* **table:** clip objects to crop ([b213bb1](https://github.com/harubi/bolivar/commit/b213bb197be9fdb174da9baa662728dcaa234dc9))
* **table:** keep exact edge keys ([f4ee52b](https://github.com/harubi/bolivar/commit/f4ee52bbcea65f68e19b4f3d3ad51eed16326d1e))
* **table:** use mediabox height ([906f2c1](https://github.com/harubi/bolivar/commit/906f2c1f311500ddf2204a308e690fb6691785d8))
* **tests:** add Python 3.10 in maturin profile test ([6f2c322](https://github.com/harubi/bolivar/commit/6f2c32230370859875816b8f1e875e1920bffb7a))
* **uniffi:** align page iteration with page tree ([2f051b8](https://github.com/harubi/bolivar/commit/2f051b8477d41aa7e9b14c2c14e28c22c22674de))
* **xref:** guard `parse` ([9cd56e1](https://github.com/harubi/bolivar/commit/9cd56e1d6b92114663163cebba2d58c842dd1d22))

### Performance Improvements

* **arena:** bump-allocate ArenaPage items ([95a3767](https://github.com/harubi/bolivar/commit/95a3767092bffcd77efcdc4d0388c82f9a06d98c))
* **arena:** bump-allocate figures and images ([a13e4c9](https://github.com/harubi/bolivar/commit/a13e4c9c683afe4886bd552cf1d89542ddc5fd47))
* **bench:** better benchmarking ([f19460d](https://github.com/harubi/bolivar/commit/f19460dafcc10dd82f6a46f2a8f2e70d8ee8b691))
* **codec:** SIMD ASCII85/ASCIIHex decode ([5625ff0](https://github.com/harubi/bolivar/commit/5625ff0ec1f8415867ee0b8a6ca6f9e352dfd9a1))
* **core:** alloc-guard + zero-alloc plane search ([a58b079](https://github.com/harubi/bolivar/commit/a58b079a9c1c5d9039874f4fee206651a2fc42d9))
* **core:** lazy page stream loading ([80d8153](https://github.com/harubi/bolivar/commit/80d8153405c99a001d4904866e7fd3fac83bff54))
* **core:** SIMD bbox union helpers ([41b9a29](https://github.com/harubi/bolivar/commit/41b9a29f4b381db43b37eed1f7488197fbf20c58))
* **core:** SIMD png predictor ([566753c](https://github.com/harubi/bolivar/commit/566753ca9fc68f1643d78d6ba576b574e0f75c18))
* **document:** simd scan for startxref/endstream ([57c929b](https://github.com/harubi/bolivar/commit/57c929b7799ea568af5d2adc5fae4cfab1a0c448))
* **layout:** add SoA SIMD overlap detection for grouping ([771a69f](https://github.com/harubi/bolivar/commit/771a69f15e3f533eeb40675c5105df0324c33654))
* **layout:** arena-backed analysis + faster `get_text` ([cc22f33](https://github.com/harubi/bolivar/commit/cc22f3399e2de9d185d5b12ef97b3e829e4179ac))
* **layout:** keep spatial-tree indices only in leaves ([73a07b8](https://github.com/harubi/bolivar/commit/73a07b8b3173e78de00c95aed342ba2a22f3ba0a))
* **layout:** neg‑max AABB SoA overlap ([2dcc898](https://github.com/harubi/bolivar/commit/2dcc89857e8e7da9cc2c5a68174c4a45db7904f4))
* **layout:** pre-reserve grouping vectors ([61e7415](https://github.com/harubi/bolivar/commit/61e74158c62fcbf6c4949bc297c157148187e148))
* **layout:** precompute metrics in LayoutSoA ([c3fc3e0](https://github.com/harubi/bolivar/commit/c3fc3e08dd41331323e6727d60d175c30a65a570))
* **layout:** remove simd bbox union helpers ([efc6fc4](https://github.com/harubi/bolivar/commit/efc6fc43a24fe6955ac0f3c017a2ca5fba0da70a))
* **layout:** reuse LayoutSoA metrics in grouping ([ca0b85b](https://github.com/harubi/bolivar/commit/ca0b85b7090102be72c8466a3daee1ba587474ac))
* **layout:** route grouping through LayoutSoA ([0f24f54](https://github.com/harubi/bolivar/commit/0f24f54e3d6e446eb890a600a19fc54ca8963dd9))
* **layout:** SIMD bbox union in group_textboxes_exact ([5386dc2](https://github.com/harubi/bolivar/commit/5386dc280e7302792a6f310f300c00b0439ae23f))
* **layout:** SIMD group_objects pair flags ([93d6c65](https://github.com/harubi/bolivar/commit/93d6c659395a69cd8db28afe74d898ee6fa7ee8c))
* **layout:** switch to single-heap best-first ([315e166](https://github.com/harubi/bolivar/commit/315e16673f555056be971f9bf80f05d1af8c6c28))
* **layout:** zero-alloc plane query ([9bcf517](https://github.com/harubi/bolivar/commit/9bcf51715a753d49b4fccf075a35fc3384e1db2d))
* new table extraction pipeline ([0e97758](https://github.com/harubi/bolivar/commit/0e977586d4f16689e82a535b9d1d08362c0ae3b6))
* **parser:** fast number parsing ([e547c70](https://github.com/harubi/bolivar/commit/e547c70c2d4f755b4a25715513330b2c8cda19b5))
* **parser:** remove simd scans from tokenizer ([1f7d715](https://github.com/harubi/bolivar/commit/1f7d715a5680cd8e90a0edaedcceb3c0323f923c))
* **parser:** simd keyword-end scan in lexer ([ecf28f3](https://github.com/harubi/bolivar/commit/ecf28f3b4d40df79e6a27f77a68b624e091fec40))
* **parser:** simd scan literal string specials ([e5f0b1c](https://github.com/harubi/bolivar/commit/e5f0b1cedd3f8fecb8d21890c0413d349eef0c1f))
* **parser:** SIMD whitespace scan in PSBaseParser ([c4c37b6](https://github.com/harubi/bolivar/commit/c4c37b65175f409732d6665be1174b53ea7e9a50))
* **python:** defer LTPage wrapping ([900bc2b](https://github.com/harubi/bolivar/commit/900bc2bcc41013624adb0897de50a0fd5fe77589))
* **python:** lazy page loading in Python shim ([7377cf8](https://github.com/harubi/bolivar/commit/7377cf8b0868b735ba9ba6b207524b0062d1f5fe))
* **table:** add AoSoA active bucket for intersections ([7c65083](https://github.com/harubi/bolivar/commit/7c650836736a8c73e18f8c123fe5f7b1b897fe9d))
* **table:** add AoSoA intersections ([76bf0c4](https://github.com/harubi/bolivar/commit/76bf0c4121bfe449e37c14dfc798ab9f201616e7))
* **table:** add caller-controlled probe policy ([56d2b03](https://github.com/harubi/bolivar/commit/56d2b03c6640088abcc2bbb553dee67b7d1a99ce))
* **table:** arena table collector ([c6be8c7](https://github.com/harubi/bolivar/commit/c6be8c7e2981e9c2b828f86741c92a6ce411f600))
* **table:** batch geometry transforms ([02d3f09](https://github.com/harubi/bolivar/commit/02d3f09f1e94964e613f772c2b55b8fc5594226f))
* **table:** bucketed AoSoA sweep for intersections ([1a9e037](https://github.com/harubi/bolivar/commit/1a9e037b991eddc74c932aeaa21982ca981f0d84))
* **table:** lazy char text materialization ([87c3790](https://github.com/harubi/bolivar/commit/87c3790e4d4a6a53ad26f314ca684d305ffb4252))
* **table:** pack AoSoA blocks for full lanes ([dbe029d](https://github.com/harubi/bolivar/commit/dbe029d284e778253f374d27d267c860d13d55ca))
* **table:** remove edge clones with typed ids ([af0d6d8](https://github.com/harubi/bolivar/commit/af0d6d8c893f8ae7b3e405620be0d55ae5b4509f))
* **table:** SIMD sweep-line + cell scan ([fe00830](https://github.com/harubi/bolivar/commit/fe008307d6373e5f078be5de30539a85c32d0daa))
* **table:** SoA cell matching ([454a669](https://github.com/harubi/bolivar/commit/454a66924af0099b3925b51522151f7f9d785f4b))
* **table:** use swap-and-pop for active edge removal ([4aa760f](https://github.com/harubi/bolivar/commit/4aa760f9366336e1de9c9fe6d9c1b7d1d127327d))
* **utils:** add batched rect transform ([7fca4a1](https://github.com/harubi/bolivar/commit/7fca4a1b84756e6b2d4fa868809f43f97b3e5d8f))
* **utils:** SIMD apply_matrix_rect ([b74c6c3](https://github.com/harubi/bolivar/commit/b74c6c3162be1ada165316d5dfe098ec34be6220))
