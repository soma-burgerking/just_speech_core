# just_speech_core

앱(`just_speech_app`)과 웹(`just_speech_web`)이 **같은 구현 하나를 함께 쓰는 코드**를 모아 둔 저장소다. Rust로 한
번 쓰고, 앱은 FFI로, 웹은 wasm으로 쓴다.

| 기능 | 구현 | 쓰는 곳 |
| --- | --- | --- |
| `slides/` — 발표 파일(pptx·ppt·odp·pdf)의 슬라이드 수·제목·발표자 노트 | `rust/`, `wasm/`(웹용 cdylib) | 앱(FFI), 웹(wasm) |
| `bundle/` — `.justspeech` 발표 묶음 읽기·쓰기, 형식은 `bundle/JUSTSPEECH_FORMAT.md` | `rust/` | 앱(FFI) |

## 무엇을 여기에 두나

한 언어로 구현해 여러 곳이 그대로 가져다 쓰는 코드만 둔다. 같은 규칙을 언어마다 따로 구현한 코드(Dart와
TS로 각각 쓴 대본 규칙 같은 것)는 두지 않는다 — 그것은 각 저장소에 있고, 두 구현을 맞추는 것은 그 저장소들의
몫이다. 서버는 이 저장소를 쓰지 않는다.

기능은 기능별 디렉터리 아래에 언어(또는 빌드 대상)별로 둔다: `<기능>/rust`, 웹용이면 `<기능>/wasm`.

## 쓰는 쪽

- 앱·웹은 이 저장소를 submodule `core/`로 둔다(URL은 상대 경로 `../just_speech_core.git`). 공개 저장소라 인증이
  필요 없다.
  - 앱: `rust/Cargo.toml`의 경로 의존(`../core/<기능>/rust`).
  - 웹: `<기능>/wasm/build.sh <just_speech_web 경로>`가 설치한 산출물을 커밋해 쓴다 — 웹의 빌드·테스트에는 Rust
    툴체인이 필요 없다.
- 여기를 바꾸면 쓰는 쪽을 올린다: 앱의 submodule 커밋, 웹은 submodule 커밋과 산출물 재설치.

## 규칙

- 앱 전용 의존성을 넣지 않는다: 모든 crate는 `wasm32-unknown-unknown`으로 빌드돼야 한다(`slides/rust`는
  `slides/wasm`이 켜는 getrandom backend가 있어야 빌드된다).
- 커밋 전에 CI(`.github/workflows/test.yml`)와 같은 것을 통과시킨다: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, 그리고 wasm 빌드.
- 주석은 코드가 스스로 말하지 못하는 제약만 영어로 적고, 변경 이력은 쓰지 않는다. 문서는 현재 상태만 적는다.
