# just_speech_core

앱(`just_speech_app`)과 웹(`just_speech_web`)이 함께 쓰는 Rust 로직. 앱은 submodule `core/`의 경로 의존으로
가져와 FFI로 쓰고, 웹은 `slides/wasm`이 만든 산출물을 쓴다. 같은 계약을 언어마다 따로 구현하지 않으려고 둔
저장소다 — 클라이언트가 함께 따라야 하는 로직은 여기에 한 번만 구현한다.

- `slides/rust` — 발표 파일(pptx·ppt·odp·pdf)의 슬라이드 수·제목·발표자 노트 읽기.
- `bundle/rust` — `.justspeech` 발표 묶음 읽기·쓰기. 형식은 `bundle/JUSTSPEECH_FORMAT.md`가 기준이다.
- `slides/wasm` — 웹용 cdylib. 자체 workspace라 크기 최적화 release profile이 앱 빌드에 닿지 않는다.
  `slides/wasm/build.sh <just_speech_web 경로>`로 웹에 설치한다 — 웹 저장소에는 산출물만 있다.

## 규칙

- **앱 전용 의존성을 넣지 않는다.** 모든 crate는 `wasm32-unknown-unknown`으로 빌드돼야 한다(`slides`는
  `slides/wasm`이 켜는 getrandom backend가 있어야 빌드된다).
- 커밋 전에 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`를 통과시킨다. CI(`.github/workflows/test.yml`)가 같은 것과 wasm 빌드를 돌린다.
- 여기를 바꾸면 쓰는 쪽을 올린다: 앱은 submodule 커밋을, 웹은 `slides/wasm/build.sh`로 산출물을 다시 설치한다.
  출력이 바뀌는 변경은 두 쪽을 함께 올려야 앱과 웹이 같은 답을 낸다.
- 주석은 코드가 스스로 말하지 못하는 제약만 영어로 적고, 변경 이력은 쓰지 않는다. 문서는 현재 상태만 적는다.
