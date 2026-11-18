# 호환성 수정본 2 — 반영 및 검증 기록

검증일: 2026-11-19, Asia/Seoul
기준 소스: `claude-code-proxy` 0.1.48에 적용한 첫 번째 호환성 개조본

## 반영 결과

| 재검토에서 발견한 항목 | 수정 후 확인 |
| --- | --- |
| 정상 `custom` 도구를 거절 | 생략·`function`·`custom` 도구 모두 수용 |
| `output_config.format: null` 거절 | 형식 미지정으로 처리, effort 유지 |
| 사용자 도구 결과 내부 참조의 이름 손실 | 현재 도구 정의와 연결된 이름을 텍스트로 보존 |
| 잘못된 참조의 조용한 손실 | 누락·빈 이름·미등록 이름·잘못된 객체 형식을 명시적 오류로 거절 |
| advisor 복구 문구 누락 | `Input tag 'advisor_20260301'` 표시 포함 |
| Nginx의 대소문자 무시 토큰 비교 | 토큰은 정확히 비교, 인증 방식 이름만 대소문자 무시 |
| 긴 키에서 map hash 오류 | 64자리 hex 키로 설정 검사 통과 |
| 로컬 인증 오류가 HTML | 로컬 401/403을 Anthropic JSON으로 반환 |
| 유효한 키의 LAN 거부가 로그에서 누락 | 로컬 403 기록, 공급자 401/403은 제외 |
| 문서 링크가 경로 문자열 파일로 바뀜 | 실제 심볼릭 링크로 복원하고 ZIP 메타데이터 보존 |

## 실행 환경과 결과

| 검사 | 결과 |
| --- | --- |
| Linux Rust 테스트 빌드 | 성공 |
| 전체 Rust 검사 | **1,192개 통과, 실패 0개, 무시 0개** |
| 기존 호환성 8개 + 새 계약 검사 11개 | **19개 모두 통과** — 전체 수에 포함 |
| Nginx HTTPS·인증·오류·로그·SSE 검사 | **31개 사례 모두 통과** |
| ZIP 문서 링크 | 심볼릭 링크 타입과 대상 정보 보존 확인 |

Rust/Cargo 1.100.0과 Nginx 1.26.3을 Debian 13 / Linux x86_64에서 사용했사와요. 전체 Rust 검사는 `--locked -- --test-threads=1`로 실행했답니다. 자세한 범위와 명령은 `CC_COMPAT_VALIDATION.md`에 있사와요.

Rust 검사는 공급자 계정을 사용하지 않는 합성 요청 및 모의 HTTP/WebSocket 서버로 실행했사와요. Nginx 검사는 임시 TLS 인증서를 신뢰하는 클라이언트를 사용했고, 리스너와 업스트림은 loopback으로 제한했답니다. 시험용 프로세스와 임시 인증서·키는 종료 시 정리했사와요.

## 실행 명령

```sh
cargo test --locked --test claude_code_compat --test claude_code_contract -- --test-threads=1
cargo test --locked -- --test-threads=1
python3 tests/nginx_gateway.py --nginx /path/to/nginx --output result.json
```

새 계약 검사는 `tests/claude_code_contract.rs`에 11개가 들어 있답니다. 주요 입력은 Codex full/lite 양쪽에서 확인하며, 전체 요청에 우연히 도구 이름이 들어 있다는 사실 대신 실제 `FunctionCallOutput`의 결과를 검증하와요. 혼합 결과에서는 텍스트·참조·지원 이미지의 순서, 오류 표시와 호출 ID도 함께 검사했사와요.

Nginx의 31개 사례에는 정확한 키, 토큰 대소문자 변경, 빈·미교체 키, 접두·접미 문자열, 중복 인증 헤더 거절, 로컬 JSON 오류와 요청 ID, 공급자 오류 보존, 실패 전용 로그, 인증 헤더 제거 및 SSE 첫 이벤트 조기 전달이 포함되어 있답니다.

## Git 이력 통합 이후 재검증 (2026-11-19)

- 원본 Git HEAD: `fe10718` (`main`, `origin/main`), 전체 기존 커밋 688개.
- 새 브랜치: `feat/claude-code-compat-gateway`; 기존 이력과 `fe10718` gzip 요청 압축 기능 보존.
- 이번 통합 소스에서 `python3 tests/nginx_gateway.py` 재실행: **31개 모두 통과**.
- `git diff --check` 통과, Git 소스 추적 및 기록 복원 확인.
- **이전 1,192개 Rust 통과는 앞선 R2 소스 스냅샷 검증에서 나온 수치**이며, 이번 Git 결합 HEAD에서 Rust/Cargo를 새로 실행한 결과는 아닙니다. 통합 환경에 Rust/Cargo가 설치되지 않아 미실행 상태랍니다.
- 실제 OAuth 계정이나 실제 Claude Code를 사용하는 종단 간 검사는 아직 수행하지 않았사와요.

## 검증의 경계

- 실제 Claude Code 클라이언트와 개인 OAuth 계정의 전체 연결은 별도 실환경 확인이 남아 있사와요.
- macOS와 Windows의 네이티브 빌드·실행 결과는 이번 Linux 검사에 포함되지 않사와요.
- advisor는 인식 가능한 미지원 오류를 반환하는 범위이며 실행 기능을 구현한 것은 아니랍니다.
- `strict: true`, native Anthropic tool search, thinking·토큰 예산·캐시 등의 기존 의미 차이는 README에 명시되어 있사와요.
- Nginx JSON 변환은 게이트웨이 자체 401/403에 적용되며, HTTP 코어가 먼저 거절하는 잘못된 요청이나 모든 다른 오류 상태를 포괄하지는 않사와요.

이 검증 기록의 대상은 수정된 소스와 배포 예제이며, 기존에 제공된 0.1.47 바이너리는 대상에 포함되지 않사와요.
