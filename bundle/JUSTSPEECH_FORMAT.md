# `.justspeech` 발표 묶음 형식

발표 하나를 파일 하나로 옮기는 **교환 포맷**이다. 내보낸 시점의 사본이며, 열면 항상 **새 발표**로 열린다
(기존 발표를 덮어쓰지 않는다 — 파일이 기존 행의 두 번째 저자가 되면 안 된다). 앱은 이 파일을 고쳐 쓰지
않으므로, 모르는 내용을 다시 저장할 때 보존해야 한다는 요구는 없다.

| 항목 | 값 |
| --- | --- |
| 확장자 | `.justspeech` |
| MIME | `application/vnd.justspeech.presentation+zip` |
| UTI | `kr.justspeech.presentation` (`public.data`, `public.content`를 따름) |

UTI가 `public.zip-archive`를 따르지 않는 이유: 따르면 파일 앱·Finder가 "압축 풀기"를 제안하고, 앱이
없는 기기에서는 내부 파일이 그대로 풀려 나온다. OS에는 불투명한 문서로 보여야 한다.

## 컨테이너

ZIP이다. 엔트리는 무압축(stored) 또는 deflate만 쓰고, ZIP 암호화는 쓰지 않는다.

**첫 엔트리는 `mimetype`이다.** 무압축, extra field 길이 0, 내용은 위 MIME 문자열의 ASCII 바이트(줄바꿈
없음). 그래서 파일 앞부분만으로 판별할 수 있다:

| 오프셋 | 내용 |
| --- | --- |
| 0 | `PK\x03\x04` |
| 30 | `mimetype` |
| 38 | `application/vnd.justspeech.presentation+zip` |

Android는 메신저·메일에서 받은 파일을 확장자 없는 `content://`와 `application/octet-stream`으로 넘기는
일이 많아서, 확장자가 아니라 이 바이트로 우리 파일인지 판단한다. 컨테이너 자체가 바뀌는 변경(예: 전체 암호화)은
MIME 문자열을 바꿔 구분한다.

엔트리 이름은 UTF-8, 구분자 `/`의 상대 경로다. 빈 세그먼트(절대 경로 포함)·`.`·`..` 세그먼트, 역슬래시·`:`·NUL,
중복 이름이 하나라도 있으면 파일을 거부한다. `/`로 끝나는 디렉터리 엔트리는 압축 도구가 넣으므로 허용한다.
읽는 쪽은 엔트리 이름으로 디스크에 풀지 않는다 — 읽은 part를 자기가 정한 경로(예: 로컬 발표 폴더의
`deck.pdf`)에 쓴다.

## `manifest.json`

내용의 목차다. 읽는 쪽은 경로를 하드코딩하지 않고 manifest의 `type`으로 part를 찾는다. 상한 1MB.

```json
{
  "formatVersion": 1,
  "bundleId": "6f1c2a9e-0b7d-4f3a-9c55-2e8d1b4a7f10",
  "createdAt": "2026-11-02T10:00:00Z",
  "generator": "JustSpeech iOS 1.8.0",
  "title": "3분기 실적 발표",
  "parts": [
    { "id": "script", "type": "script", "schema": 1, "path": "parts/script.json", "required": true },
    { "id": "deck", "type": "deck", "mediaType": "application/pdf", "path": "parts/deck.pdf", "sha256": "5288d255…" },
    { "id": "slide-mapping", "type": "slide-mapping", "schema": 1, "path": "parts/mapping.json", "refs": ["script"] }
  ]
}
```

| 필드 | 필수 | 설명 |
| --- | --- | --- |
| `formatVersion` | O | 정수. 모르는 part를 무시하는 규칙으로 감당할 수 없는 변경에만 올린다 — 올리면 이전 앱은 파일을 통째로 거부한다. |
| `bundleId` | O | 내보낼 때마다 새로 만드는 UUID. 같은 파일을 다시 받았는지 알아볼 근거. |
| `createdAt` | O | RFC 3339 UTC. |
| `generator` | | 진단용 문자열. 읽는 쪽은 이 값으로 동작을 바꾸지 않는다. |
| `title` | O | 발표 제목, 255자 이하(서버 `title` 상한). 빈 문자열이면 제목 없음. |
| `parts` | O | part 목록. |

**part**

| 필드 | 필수 | 설명 |
| --- | --- | --- |
| `id` | O | 묶음 안에서 유일, `[a-z0-9-]{1,64}`. `refs`가 가리키는 이름. |
| `type` | O | part 종류. 아래 "part 종류". |
| `path` | O | 엔트리 이름. |
| `schema` | JSON part | 그 part의 스키마 정수. 같은 `schema` 안에서는 필드 추가만 한다. |
| `mediaType` | 바이너리 part | 예: `application/pdf`. |
| `required` | | 기본 `false`. 이 part 없이 가져오면 발표의 의미가 달라질 때만 `true`. |
| `refs` | | 이 part가 기대는 part의 `id`. 하나라도 건너뛰면 이 part도 건너뛴다. |
| `sha256` | | 있으면 읽는 쪽이 검증한다. |

## 읽는 쪽 규칙

확장성은 전부 이 규칙에서 나온다. 새 기능은 새 part `type`(또는 `schema`)으로 추가하고, 이전 앱은 그것을
건너뛴 채 나머지를 가져온다.

1. 첫 엔트리가 `mimetype`이고 내용이 위 MIME과 같아야 한다.
2. `formatVersion`이 아는 값보다 크면 거부한다 → "앱 업데이트 필요".
3. manifest와 part 객체의 모르는 필드는 무시한다.
4. 각 part가 **아는 `type`이고, `schema`가 아는 값 이하이고, `mediaType`을 지원**하면 읽을 대상이다.
   아니면 `required: true`일 때 거부("앱 업데이트 필요"), 아니면 건너뛴다. 건너뛴 part는 압축을 풀지 않는다.
5. `refs` 중 건너뛰었거나 없는 part가 있으면 그 part도 건너뛴다(같은 `required` 규칙).
6. 읽을 part는 아래 상한을 넘는 순간 읽기를 멈춘다(엔트리 헤더의 크기 값은 믿지 않는다). 상한 초과나 내용
   검증 실패는 `required` part면 거부("손상된 파일"), 아니면 그 part만 건너뛴다.
7. v1 `type`이 둘 이상 있으면 거부한다.

사용자에게 보이는 거부 사유는 "앱 업데이트 필요"와 "손상된 파일" 두 가지뿐이다.

## part 종류 (v1)

### `script` — 대본 (`schema` 1, `required: true`)

```json
{ "text": "안녕하세요. 오늘은 3분기 실적을 말씀드리겠습니다. …" }
```

- `text`: 마커(`{{N}}`)를 걷어낸 저장용 대본. 공백만으로 이루어지면 안 된다(서버 `scriptText` 규칙과 같다).
- 상한 2MB.

### `deck` — 발표 자료 (`mediaType` `application/pdf`)

- 슬라이드 렌더로 만든 PDF. 쪽마다 렌더 이미지 하나이고, 쪽 크기는 렌더 DPI(150)로 정해 다시 렌더하면 같은
  픽셀이 나온다. 원본 문서는 넣지 않는다(아래 "넣지 않는 것"). 렌더 이미지와 썸네일을 따로 넣지도 않는다 —
  여는 쪽이 로컬 발표와 같은 방식으로 다시 렌더한다.
- 상한 100MB, 200쪽(렌더 파이프라인 상한과 같다). 쪽수는 렌더할 때 검사한다.
- 다른 `mediaType`은 v1에서 지원하지 않는다(규칙 4로 건너뛴다).

### `slide-mapping` — 슬라이드 매칭 (`schema` 1, `refs: ["script"]`)

```json
{
  "tokenUnit": "whitespace",
  "mappings": [
    { "startToken": 0, "endToken": 41, "slide": 1, "title": "인사" },
    { "startToken": 42, "endToken": 97, "slide": 2, "disabled": true }
  ]
}
```

- `mappings`의 항목은 서버 `SlideMapping`과 같은 모양·같은 상한이다: 최대 200개, `slide` 1..200, `title`
  255자 이하, `startToken`·`endToken`은 0부터 세는 닫힌 범위.
- `tokenUnit`은 토큰 인덱스의 기준이다. `whitespace`는 `script.text`를 ECMAScript `\s+`로 자르고 빈
  조각을 버린 순번이다(서버와 `countScriptTokens`가 세는 방식). 모르는 값이면 이 part를 건너뛴다 — 토큰을
  세는 방식이 다른 매핑을 그대로 쓰면 경계가 조용히 어긋난다.
- 상한 1MB.

## 넣지 않는 것

| 내용 | 이유 |
| --- | --- |
| 원본 발표 문서(PPTX·원본 PDF) | 파일은 발표하기 위한 것이다. 원본의 글자·발표자 노트·숨긴 슬라이드·편집 가능한 내용은 소유자에게 남는다. |
| 렌더 이미지·썸네일 | PDF에서 다시 만든다. |
| 녹음·발표 기록 | 기기에만 있는 개인 데이터. |
| 즐겨찾기·accent | 받는 사람의 표시 설정. |
| `remoteId`·`syncSerial`·서버 복사본 메모 | 서버 쪽 정체성. 가져온 발표는 새 발표다. |
| 발표별 설정(목표 시간 등) | v1에는 없다. 넣게 되면 선택 part로 추가한다. |

## 쓰는 쪽 규칙

- `mimetype`을 첫 엔트리로, 무압축으로 쓴다. PDF는 이미 압축돼 있으므로 무압축으로 쓴다.
- 내보낼 때마다 `bundleId`를 새로 만든다.
- 덱은 원본이 아니라 그 기기가 가진 슬라이드 렌더로 만든다. 빠진 쪽이 있으면 덱을 쓰지 않는다 — 구멍 난 덱은
  뒤 슬라이드의 매핑을 전부 밀어낸다.
- `required`는 `script`에만 붙인다.

## 구현과 호환성 검증

리더·라이터는 `bundle/rust`다. 앱과 웹이 같은 crate를 쓰므로 위 규칙의 구현은 하나다.

`formatVersion`·`schema`마다 라이터가 실제로 쓴 파일을 `bundle/rust/tests/fixtures/`에 남기고, 읽는 쪽은
모든 fixture를 열어야 한다. fixture는 추가만 하고 다시 만들지 않는다. 미래 버전을 흉내 낸 파일(모르는 선택
part, 모르는 필수 part, 큰 `schema`)은 단위 테스트가 직접 조립해 읽는 쪽 규칙을 고정한다.
