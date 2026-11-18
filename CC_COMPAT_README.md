# Claude Code / Codex LAN 게이트웨이 — 호환성 수정본 2

**기준:** 원본 Git 저장소 `main`의 `fe10718` (패키지 버전 0.1.48, 기존 이력 688개)를 바탕으로 생성한 `feat/claude-code-compat-gateway` 브랜치. 원본 커밋에 포함된 Codex gzip 요청 압축 기능은 그대로 유지했답니다.
**수정일:** 2026-11-19. 앞선 개조본을 재검토하여 확인한 회귀와 인증 오류를 반영한 소스랍니다. 원본 프로젝트의 라이선스와 고지는 유지했으며, 공식 Anthropic/OpenAI 통합이나 100% 동등한 API 구현을 뜻하지는 않사와요.

## Git 이력 및 배포 형식

이 수정본은 소스 스냅샷이 아니라 **원본 `.git` 메타데이터와 기존 커밋 이력을 보존한 저장소**랍니다. `main` 및 `origin/main`은 원본 `fe10718`에 남겨 두고, 호환성 수정과 LAN 게이트웨이 설정을 별도의 새 커밋으로 기록했사와요. 새로운 브랜치가 체크아웃된 ZIP 또는 별도로 제공하는 Git bundle에서 아래처럼 이력을 확인할 수 있답니다.

```sh
git log --oneline --decorate -7
git log --oneline main..feat/claude-code-compat-gateway
git diff --stat main..feat/claude-code-compat-gateway
```

Git bundle로 복원할 때는 `git clone claude-code-proxy-cc-compat.bundle claude-code-proxy`로 로컬 복제할 수 있답니다. 실제 원격 저장소로는 **push하지 않았사와요**. 원본 프로젝트의 `origin` 주소는 원본 ZIP에 들어 있던 값을 유지했답니다.

## 이번에 반영한 내용

| 항목 | 수정 후 동작 |
| --- | --- |
| 일반 도구 타입 | 타입 생략, `custom`, 기존 `function` 모두 수용 |
| `output_config.format: null` | 출력 형식 미지정으로 처리하며 effort 설정은 유지 |
| 중첩 `tool_reference` | 사용자 `tool_result.content`에서 이름을 보존하고 현재 `tools` 정의에 있는지 확인 |
| 잘못된 도구 참조 | 이름 누락·빈 문자열·비문자열·미등록 이름·잘못된 단일 객체 형식은 위치를 포함한 오류로 거절 |
| 혼합 도구 결과 | 참조, 텍스트, 지원하는 이미지의 순서와 호출 ID·`is_error` 표시 유지 |
| advisor 미지원 | Claude Code가 인식하는 `Input tag 'advisor_20260301'` 오류 표시 포함 |
| Nginx 키 비교 | 토큰은 대소문자를 정확히 구별하고 `Bearer` 방식 이름만 대소문자 무시 |
| 긴 키와 미설정 상태 | 64자리 lowercase hex 키 사용, 미교체·빈 placeholder는 접근 차단 |
| Nginx 자체 401/403 | Anthropic 형태 JSON, 본문과 헤더에 같은 request ID 반환 |
| 공급자 401/403 | 상태·본문·request ID를 그대로 전달하며 로컬 인증 실패로 기록하지 않음 |
| 접근 실패 로그 | 잘못된 키와 정상 키의 LAN 대역 거부를 함께 기록 |
| 패키징 | 문서의 Changelog 심볼릭 링크 복원 및 ZIP에 링크 정보 보존 |

계정 DB, 클라이언트별 키, 요청 빈도나 동시 요청 제한을 추가하지 않은 1인 LAN 구성인 것이와요.

## 검증 결과

아래 Rust 결과는 **이전 0.1.48 소스 스냅샷 개조본에서 진행한 검사 결과**를 보존한 것이랍니다. 이번 Git 이력 통합 환경에는 Rust/Cargo가 없어 전체 Rust 검사를 재실행하지 못했사와요. 반면 Nginx 31건은 현재 Git 통합 소스에서 다시 실행하여 전부 통과했답니다. 원본 `fe10718`에 새로 추가된 gzip 요청 압축 기능과 본 패치의 조합은 실제 Rust 빌드로 추가 확인해야 하와요.


| 검사 | 결과 |
| --- | --- |
| Linux Rust 테스트 빌드 (기존 R2 소스) | 성공, 이번 Git 결합에서는 미실행 |
| 전체 Rust 검사 (기존 R2 소스) | **1,192개 통과, 실패 0개, 무시 0개** — 이번 Git 결합에서는 미실행 |
| 기존 호환성 8개 + 새 계약 검사 11개 | **19개 모두 통과** — 전체 수에 포함 |
| Nginx HTTPS·인증·오류·로그·SSE 검사 | **현재 Git 결합 소스에서 31개 사례 모두 통과** |
| ZIP 문서 링크 | 심볼릭 링크 타입과 대상 정보 보존 확인 |

이전 Rust 검증에는 Rust/Cargo 1.100.0을 Debian 13 / Linux x86_64에서 사용했고, 이번 Git 통합 Nginx 재검증에는 Nginx 1.26.3을 사용했사와요. 전체 Rust 검사는 `--locked -- --test-threads=1`로 실행했답니다. 자세한 범위와 명령은 `CC_COMPAT_VALIDATION.md`에 있사와요.

검사는 합성 요청과 loopback 모의 서버를 사용했사와요. **macOS/Windows 네이티브 실행과 실제 Claude Code → OAuth → 모델까지의 전체 연결은 별도 실환경 확인이 남아 있답니다.** 기존 0.1.47 릴리스 실행 파일에는 이 변경이 들어 있지 않으므로, 이 소스로 대상 OS에서 빌드한 실행 파일을 사용해야 하와요.

## 빌드와 검사

프로젝트 루트에서 현재 Rust stable과 대상 OS의 컴파일 도구를 사용하면 된답니다. 이번 검증에는 Linux x86_64의 Rust/Cargo 1.100.0을 사용했사와요.

```sh
cargo test --locked --test claude_code_compat --test claude_code_contract -- --test-threads=1
cargo test --locked -- --test-threads=1
cargo build --locked --release
```

macOS Apple Silicon에서 직접 빌드하면 `target/release/claude-code-proxy`, Windows의 Rust/MSVC 환경에서는 `target/release/claude-code-proxy.exe`가 생성된답니다. Mac ARM64의 타깃은 `aarch64-apple-darwin`, Windows x64의 타깃은 `x86_64-pc-windows-msvc`랍니다.

Nginx 검사는 Python 3.9+, Nginx, OpenSSL이 있는 환경에서 실행할 수 있사와요.

```sh
python3 tests/nginx_gateway.py --nginx /path/to/nginx --output gateway-test.json
```

Nginx가 PATH에 있으면 `--nginx`를 생략할 수 있답니다. 이 검사는 임시 인증서를 신뢰하는 테스트 클라이언트와 loopback 리스너만 사용하며, 생성한 프로세스와 임시 키·설정은 종료 시 정리하와요. 결과 JSON에는 시험 결과가 남는답니다.

## 서버 설정

구조는 **Claude Code → LAN의 Nginx HTTPS → 127.0.0.1의 프록시 → Codex OAuth**랍니다. OAuth 자격증명은 프록시를 실행하는 서버 계정에서 관리하와요.

### 1. 서버에서 OAuth 로그인과 프록시 실행

macOS/Linux의 빌드 결과를 사용하는 예시랍니다.

```sh
./target/release/claude-code-proxy codex auth login
CCP_BIND_ADDRESS=127.0.0.1 ./target/release/claude-code-proxy serve --port 18765 --no-monitor
```

Windows PowerShell에서는 빌드한 `.exe`를 같은 방식으로 실행하고, 바인드 주소를 환경 변수로 설정하면 된답니다.

```powershell
.\target\release\claude-code-proxy.exe codex auth login
$env:CCP_BIND_ADDRESS = '127.0.0.1'
.\target\release\claude-code-proxy.exe serve --port 18765 --no-monitor
```

### 2. 로컬 API 키 생성

```sh
openssl rand -hex 32
```

출력되는 **64자리 lowercase hex 문자열**을 Nginx 설정의 `REPLACE_WITH_64_HEX_SECRET`에 넣고, 클라이언트의 `ANTHROPIC_AUTH_TOKEN`에도 같은 값을 넣으면 된답니다. 이 설정은 키를 정규식에 삽입하므로, 지정한 문자 집합과 길이를 유지해야 하와요. 교체 전 placeholder나 빈 키로는 인증되지 않사와요.

### 3. Nginx 배포 예제 적용

`deploy/nginx/claude-code-proxy-lan.conf`에서 다음 값을 실제 환경에 맞추면 된답니다.

- `192.168.0.29:18766`: 서버의 LAN IP와 공개할 LAN 포트.
- `192.168.0.0/24`: 접근을 허용할 실제 LAN 대역.
- 인증서와 개인키 경로: 클라이언트가 신뢰할 HTTPS 인증서.
- `REPLACE_WITH_64_HEX_SECRET`: 앞에서 생성한 단일 로컬 API 키.
- `logs/ccp-auth-fail.log`: Nginx prefix를 기준으로 하는 기록 경로.

이 파일의 `map`, `log_format`, `server`는 모두 Nginx의 `http` 블록 안에 들어가야 하와요. 설정 후 `nginx -t`로 확인하고 해당 Nginx 인스턴스에 반영하면 된답니다. 실제 배포 설정을 바꾸는 과정은 서버 환경에서 진행해야 하와요.

IP 주소를 HTTPS endpoint로 사용한다면 인증서 SAN에 그 IP가 포함되어야 하며, 클라이언트 런타임이 발급 CA를 신뢰하도록 구성해야 한답니다. TLS 검증을 끄는 설정으로 대신하지 않는 것이와요.

### 4. Claude Code 연결

아래 placeholder를 실제 값으로 바꾸면 된답니다. `YOUR_CODEX_MODEL_ID`에는 이 프록시와 해당 계정에서 사용할 수 있는 모델 ID를 넣어야 하와요.

```sh
ANTHROPIC_BASE_URL='https://SERVER_LAN_IP:18766' \
ANTHROPIC_AUTH_TOKEN='REPLACE_WITH_64_HEX_SECRET' \
ANTHROPIC_MODEL='YOUR_CODEX_MODEL_ID' \
CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \
CLAUDE_CODE_DISABLE_NONSTREAMING_FALLBACK=1 \
claude
```

인증은 `Authorization: Bearer ...` 방식이랍니다. `x-api-key`만 전송하는 클라이언트는 이 예제에서 인증되지 않으므로, Claude Code에서는 `ANTHROPIC_AUTH_TOKEN`을 사용하면 된답니다. Nginx는 인증 후 두 인증 헤더를 모두 제거하고 프록시에 요청을 전달하와요.

## 실패 로그와 오류 응답

`ccp-auth-fail.log`에는 Nginx가 직접 발생시킨 401/403이 남는답니다. 정상 키를 가지고도 허용 대역 밖에서 접근한 403 역시 포함되며, 공급자에서 발생한 401/403은 제외하와요.

전용 로그의 필드는 시각, 출발지 IP, 메서드, 경로, HTTP 상태, upstream 상태, request ID랍니다. 인증 헤더·본문·쿼리 문자열을 기록하지 않사와요. URL 경로 자체에 민감한 값을 넣으면 경로에는 남을 수 있으며, 별도의 Nginx `error_log`에는 전체 URI가 기록될 수 있다는 기존 제한도 유지되와요.

게이트웨이 자체의 인증 오류는 다음 구조랍니다. `request-id` 헤더와 본문의 `request_id`는 동일하와요.

```json
{
  "type": "error",
  "error": {
    "type": "authentication_error",
    "message": "Invalid gateway API key"
  },
  "request_id": "GATEWAY_REQUEST_ID"
}
```

LAN 대역 거부에는 `permission_error`와 403을 반환하와요. 공급자의 오류는 가로채서 새 메시지로 바꾸지 않고 그대로 전달한답니다. Nginx 자체 HTTP 파서가 먼저 거절하는 중복 Authorization 같은 잘못된 HTTP 요청까지 모든 오류가 Anthropic JSON이 되는 것은 아니랍니다.

## 유지되는 호환성 한계

이번 수정은 실제 회귀와 누락된 경로를 바로잡은 것이며, 서로 다른 모델 서비스의 의미까지 동일하게 만드는 것은 아니랍니다.

| 기능 | 현재 범위 |
| --- | --- |
| `strict: true` 도구 | 명시적 미지원으로 거절. 무시하면서 지원한다고 표시하지 않음 |
| tool search / `defer_loading` | 도구를 미리 등록하고 검색 참조를 텍스트로 보존. native 검색 실행과 다름 |
| advisor | 실행 미지원. 인식 가능한 거절 문구로 클라이언트 복구 지원 |
| thinking / 서명 / 예산 | Anthropic과 1:1 동등하지 않음 |
| `max_tokens` | 원본의 Codex 변환에서 출력 토큰 제한으로 그대로 전달하지 않음 |
| JSON schema | 일부 pattern 제약이 완화되며 structured output의 선택 필드 의미도 달라질 수 있음 |
| 이미지·문서 | 기존 공급자 지원 범위 유지. 지원하지 않는 중첩 이미지나 블록의 기존 제한이 남음 |
| 토큰 계산·캐시·beta 기능 | Anthropic의 실제 계산 및 모든 기능과 동등하다고 보장하지 않음 |

`strict: true`는 정상 API 기능이지만 현재 브리지에서는 지원하지 않사와요. 이런 요청 때문에 Claude Code가 400을 표시한다면, 공식 문서의 `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS=1` 설정으로 전송 기능을 줄이는 방법이 있답니다. 이 설정은 기능 축소이며, 도구 검색 재시도가 strict까지 자동 제거해 준다는 의미도 아니랍니다.

공식 계약 참고 자료랍니다.

- [Claude Code 게이트웨이 호환 가이드](https://code.claude.com/docs/en/llm-gateway-protocol)
- [Anthropic Messages API](https://platform.claude.com/docs/en/api/messages/create)
- [Tool search의 custom 구현](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool)
- [API 오류 형식](https://platform.claude.com/docs/en/api/errors)
- [Nginx map 비교 규칙](https://nginx.org/en/docs/http/ngx_http_map_module.html)

원본 MIT 라이선스와 고지는 `LICENSE`에 유지되어 있사와요.
