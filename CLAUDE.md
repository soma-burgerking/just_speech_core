# just_speech_core

앱(`just_speech_app`)·웹(`just_speech_web`)·서버(`just_speech_api`)가 같은 답을 내야 하는 로직을 모아 둔
저장소다. **기능별 디렉터리 아래에 언어별 구현을 둔다.** 한 기능을 여러 언어로 구현해야 하면 그 구현들과,
그것들을 함께 묶는 테스트 데이터(`vectors/`)가 같은 디렉터리에 있다 — 한 PR에서 함께 고치고 CI가 함께 검증한다.

| 기능 | 구현 | 쓰는 곳 |
| --- | --- | --- |
| `slides/` — 발표 파일(pptx·ppt·odp·pdf)의 슬라이드 수·제목·발표자 노트 | `rust/`, `wasm/`(웹용 cdylib) | 앱(FFI), 웹(wasm) |
| `bundle/` — `.justspeech` 발표 묶음 읽기·쓰기, 형식은 `bundle/JUSTSPEECH_FORMAT.md` | `rust/` | 앱(FFI) |
| `qa-prompt/` — Q&A 답변 생성 시스템 프롬프트 | `kotlin/`, `dart/`, `vectors/` | 서버, 앱(기기 전용 발표의 직접 연결) |
| `script/` — 대본 규칙: 단어 수, 문장 수(`scriptLineCount`), 문장 끝 | `dart/`, `ts/`, `vectors/` | 앱, 웹 |

## 쓰는 쪽

- 앱·서버·웹은 이 저장소를 submodule `core/`로 둔다(URL은 상대 경로 `../just_speech_core.git`).
  공개 저장소라 쓰는 쪽 CI는 체크아웃에 `submodules: true`만 있으면 된다.
  - 앱: Rust는 `rust/Cargo.toml`의 경로 의존(`../core/<기능>/rust`), Dart는 `pubspec.yaml`의 경로 의존
    (`core/<기능>/dart`).
  - 서버: `settings.gradle.kts`의 `includeBuild("core")`. 의존은 `kr.justspeech.core:<기능>`이다. 서버 이미지는
    Docker 안에서 빌드하므로, Kotlin 구현이 있는 기능을 추가하면 서버 `Dockerfile`의 `COPY`에도 넣는다.
- 웹: TS 구현은 경로 별칭 `@core/<기능>/ts`로 그대로 가져다 쓰고, wasm은 `slides/wasm/build.sh <just_speech_web 경로>`가
  설치한 산출물을 커밋해 쓴다.
- 여기를 바꾸면 쓰는 쪽을 올린다: 앱·서버는 submodule 커밋, 웹은 산출물 재설치. 출력이 바뀌는 변경은 쓰는
  쪽을 모두 올려야 같은 답을 낸다. 서버(main = 운영)가 함께 바뀌면 서버를 먼저 배포한다.

## 규칙

- **여러 언어로 구현한 기능은 `vectors/`의 사례를 모든 구현이 통과해야 한다.** 사례는 입력과 기대 출력이다.
  어느 한 구현에서 만들어 내지 말고, 기대 출력이 맞는지 사람이 확인한 뒤 넣는다. 이미 배포된 쪽이 기대는
  출력이므로 지우거나 바꾸는 것은 계약을 바꾸는 일이다.
- 언어마다 기본 동작이 다른 것(정규식 `\s`, 대소문자, 문자열 길이 단위)은 기본값에 맡기지 말고 명시한다.
- Rust crate는 앱 전용 의존성을 넣지 않는다: 모두 `wasm32-unknown-unknown`으로 빌드돼야 한다(`slides/rust`는
  `slides/wasm`이 켜는 getrandom backend가 있어야 빌드된다).
- 커밋 전에 CI(`.github/workflows/test.yml`)와 같은 것을 통과시킨다:
  - Rust: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
    `cargo test --workspace`
  - Kotlin: `./gradlew test`
  - Dart: 각 `dart/`에서 `dart test`
  - TS: `node --test '<기능>/ts/*.test.ts'`(Node 24가 타입을 지우고 바로 돌린다 — `ts/`에는 지울 수 있는 문법만 쓴다)
- 주석은 코드가 스스로 말하지 못하는 제약만 영어로 적고, 변경 이력은 쓰지 않는다. 문서는 현재 상태만 적는다.
