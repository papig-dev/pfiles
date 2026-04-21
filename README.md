# pfiles (egui)

`rift`의 Rust 로직을 최대한 재사용하면서, Tauri 대신 `egui/eframe`으로 만든 데스크톱 파일 매니저 프로토타입입니다.

## 현재 구현

- 듀얼 패널 (좌/우) + 활성 패널 표시
- 경로 입력 이동 (`Go`)
- 폴더 상위 이동 (`Up`)
- 숨김 파일 토글
- 파일/폴더 목록 로딩 (`fs_ops` 재사용)
- 더블클릭 열기
  - 폴더: 진입
  - 아카이브: 내부 엔트리 리스트 표시 (`archive` 재사용)
  - 파일: 프리뷰 패널 로딩 (`preview` 재사용)
- 키보드 단축키
  - `Tab`: 활성 패널 전환
  - `↑/↓`: 항목 이동
  - `Enter`: 열기
  - `Backspace`: 상위 폴더
  - `Space`: 프리뷰
  - `F2`: 이름 변경
  - `F5`: 반대 패널로 복사
  - `F6`: 반대 패널로 이동
  - `F7`: 새 폴더
  - `F8`: 삭제(휴지통 옵션)
  - `⌘H`: 숨김 파일 토글
- 우측 프리뷰 패널
  - 텍스트 프리뷰
  - 이미지 프리뷰
  - 바이너리 메타 표시

## 재사용된 Rust 모듈

- `src/core/fs_ops.rs`
- `src/core/archive.rs`
- `src/core/preview.rs`

(`rift/src-tauri/src/*`에서 Tauri 의존 어노테이션을 제거해 `egui` 앱에서 직접 호출 가능하도록 정리)

## 실행

```bash
cargo run
```

## 폰트 번들 (필수)

OS별 설치 상태와 무관하게 동일 렌더링을 위해 앱 번들 폰트만 로드합니다.

아래 파일명을 `assets/fonts`에 넣어주세요:

- `assets/fonts/JetBrainsMonoHangulNerdFont-Regular.ttf`
- `assets/fonts/JetBrainsMonoHangulNerdFontMono-Regular.ttf`
- `assets/fonts/JetBrainsMonoHangul-Regular.ttf`
- `assets/fonts/JetBrainsMonoHangul-Medium.ttf`

(`.ttf`를 우선 사용하며, `.ttc`는 예비 경로로도 인식합니다)

우선순위:

1. JetBrainsMonoHangul Nerd
2. JetBrainsMonoHangul

원본 프로젝트: `https://github.com/Jhyub/JetBrainsMonoHangul`

라이선스 파일: `assets/fonts/OFL-LICENSE.txt`

## 참고

- 현재는 **로컬 파일 시스템 + 아카이브 목록/프리뷰 중심 베이스라인**입니다.
- SFTP/복사/이동/삭제/이름변경/충돌 처리/진행률 UI는 다음 단계로 확장 가능합니다.
