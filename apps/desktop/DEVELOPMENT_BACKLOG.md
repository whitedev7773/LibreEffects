# Libre Effects 추가 개발 기능과 우선순위

2026년 10월 1일, 커밋 `0ce0022`의 Rust 데스크톱 앱과 공용 코어를 기준으로 조사했다. 목적은 단순한 2D 타이틀 편집기를 실제 제작에 사용할 수 있는 After Effects 계열 작업 도구로 발전시키는 것이다. **먼저 저장과 미디어 처리의 안정성을 보강하고, 프로젝트와 속성 모델을 확장한 뒤, 프리컴포지션·오디오·합성 기능을 연결해야 한다.**

아래 내용은 코드 조사와 Adobe 공식 문서 대조를 통해 작성한 개발 제안이다. 이번 조사에서는 앱을 수정하거나 새로운 런타임 테스트를 수행하지 않았다. 현재 열려 있는 After Effects 화면을 다시 측정한 결과도 아니다. 기존 README의 화면 비교 기록은 참고하되, 픽셀 단위 UI 일치는 별도 검증 과제로 남긴다. 모든 Adobe 효과·코덱·플러그인의 개별 옵션을 망라한 명세는 아니며, 주요 제작 작업 영역의 백로그다.

## 구현 진행 상황

이 문서의 아래 조사 표는 최초 감사 시점의 기록이다. 이후 구현 상태는 이 절을 기준으로 확인한다. 나머지 항목은 아직 완료로 처리하지 않았다.

### 현재 단계와 다음 개발

현재는 **2D 모션 편집·합성 기반을 구현한 단계**다. AE 동등 수준이나 전체 백로그 완료 상태는 아니다. 기능군마다 규모가 달라 단순 항목 수를 완성률로 표시하지 않는다.

- **구현·검증됨:** 다중 컴포지션/프리컴포지션, 공유 자산·폴더·썸네일·다중 파일/이미지 시퀀스 가져오기·FPS/알파 해석, 저장·복구·미디어 수집/재연결, 유리수 FPS, Null/Solid/Adjustment, 레이어 복사·마커·스냅·다중 정렬/분배·부모 Pick Whip, 변형/효과 키 편집, 영상·프리컴포지션 Time Remap, 순서 있는 효과 스택, 5종 블렌딩·4종 Track Matte, Curves/Gradient 등 기본 효과, 배경색 MP4·알파 MOV/PNG, 뷰 상태 저장·눈금자/가이드/채널/픽셀 정보, 스냅샷 기반 렌더 큐·다중 출력·재시도·출력 크기/FPS/채널/인코딩 설정, 독립/영상 오디오 메타데이터·파형·중첩 믹싱·AAC/PCM 영상 출력, 좌우 레벨·팬·페이드 애니메이션·오디오 스위치·구간 미터, Windows 기본 장치 재생·100ms 스크럽·장치 시계 기반 플레이헤드·재생 블록 미터, 지속 영상 디코더·제한된 순차 프리페치·비동기 합성과 오래된 요청 취소. 상세 제약은 아래 표에 남긴다.
- **다음 핵심 개발(2026-10-02 사용자 우선순위 변경):** 실제 AE 첫 작업 화면을 기준으로 편집 도구·효과·텍스트·마스크와 관련 UI를 우선한다. J02 디스크 캐시/J03 증분 평가, 고급 출력·GPU·HDR·장치/코덱 확장은 뒤로 미룬다.
- **정교한 편집에 필요한 기능:** D02/D03 양방향 시간 보간·Speed Graph, E02/E03 Contents·도형 속성 애니메이션·경로 토폴로지 편집, G01 가변 Feather·로토베지어, F01–F03 직접 텍스트 편집·문자별 스타일(글꼴/실제 스타일 선택은 아래 범위로 구현), B01/B05/B07 배치 회귀·단축키/접근성; B06 공통 색 선택기는 아래 범위로 구현.
- **후속 고급 기능:** 패널 도킹·소스 뷰어·최근 프로젝트, 타임라인 고급 시간 편집/검색/가상화, 공간 경로·프리셋·표현식, Shape 연산/SVG·Text Animator, 추가 효과·모션 블러, 고정밀 색/HDR·Proxy/GPU, 템플릿, 설치/업데이트·한글화와 UI 회귀 자동화.
- **별도 대형 단계:** 3D/카메라/라이트·모델, 추적·로토/Puppet, 플러그인/교환 포맷 연구, 웹/API 제품 범위. AEP/MOGRT 호환은 구현되지 않았다.

### 페인트 Composite 앞/뒤 합성 — 2026-10-03

- E04: 단색/Gradient Fill·Stroke의 Properties에 Composite의 Below Previous/Above Previous를 추가했다. 같은 그룹에서 앞서 처리한 결과의 뒤/앞에 현재 페인트를 합성하며, Contents 항목 순서·사용하는 경로·기존 속성 키는 그대로 유지한다. 중첩 그룹은 독립 합성 범위를 유지한다. [Adobe의 Composite 설명](https://helpx.adobe.com/after-effects/desktop/drawing-painting-and-paths/shapes-and-shape-attributes/shape-attributes-paint-operations-path.html)을 참고하고 실제 AE Fill의 기본값 Below Previous in Same Group 표시를 확인했다.
- 기존 문서는 Below Previous로 읽고 기본 필드는 저장에서 생략한다. Above Previous 사용 문서만 v46을 요구한다. 잠금·사라진 항목·경로/그룹 대상으로 명령을 실행하면 원자적으로 거부하며, v45 이하로 위장한 새 합성 값도 거부한다. 한 Undo/Redo와 복제·저장 왕복으로 값을 유지한다.
- 자동 검증: 코어 187개 + 데스크톱 243개(총 430개) 통과, 외부 미디어/장치 30개 제외. 네 종류 페인트의 0/30/60프레임 반투명 겹침을 독립 합성식과 비교하고 Preview/출력 일치·중첩 그룹 범위·경로/키 순서 유지·히스토리·잠금·버전·JSON을 검사했다. Cargo check/fmt/test/release 및 git diff --check 통과(Moon/proto 미설치, 기존 릴리스 경고 17개).
- 실제 Windows에서 기존 Gradient 위의 Fill을 Above Previous로 전환·Undo/Redo하고 Fill Opacity 50%로 저장한 뒤 재열어 값과 미리보기 유지를 확인했다. 실제 저장본 `target/qa/composite-native.lfe.json`의 0/15/30/45/60프레임 출력 27,225개 샘플이 독립 source-over 계산과 2/255 이내로 일치했다(`verify_composite_native.py`). 1442/1920px 창 배치, 실제 작업 창 1개, 사용자 보존 원본 SHA256 불변을 확인했다. 다른 페인트 종류와 중첩 그룹은 자동 테스트 범위이며 네이티브 조작은 별도 미검증이다.
- 이 옵션은 정적 합성 순서다. 페인트별 Blend Mode, 경로 연산자와 Gradient Editor/Composition 손잡이는 남아 있으며 E04 전체 완료로 표시하지 않는다.

### Gradient Fill·Gradient Stroke와 독립 색/불투명도 스톱 — 2026-10-03

- E04/D01: Contents Add에 Gradient Fill/Gradient Stroke를 추가했다. Linear/Radial, Start/End, Highlight Length/Angle, 색/불투명도 스톱 각각의 위치·중간점·RGB/불투명도를 Properties에서 편집하고 스톱워치·Timeline·Graph에 연결한다. 각 종류 2–32개 스톱을 지원하며 안정된 ID로 키를 유지한다. 추가는 현재 프레임의 50% 색/불투명도를 샘플링하고 제거 Undo는 키까지 복원한다. Gradient Stroke는 Cap/Join/Dash를 함께 사용한다.
- [Adobe Shape 페인트 설명](https://helpx.adobe.com/in/after-effects/desktop/drawing-painting-and-paths/shapes-and-shape-attributes/shape-attributes-paint-operations-path.html)의 독립 색/불투명도 스톱과 방사형 Highlight 속성을 참고했다. 현재 구현은 sRGB의 거듭제곱 중간점 보간과 적응형 SVG 샘플링을 사용한다. 스톱이 같은 위치에 있으면 경계를 유지하고, 공간 순서가 교차해도 트랙 ID는 유지한다. 레이어별 SVG 정의를 분리해 서로 다른 그라디언트가 충돌하지 않게 했다. 그라디언트 사용 파일만 v45이며 기존 Contents는 v44를 유지한다.
- 자동 검증: 코어 186개 + 데스크톱 242개(총 428개) 통과, 외부 미디어/장치 30개 제외. 스톱 추가/제거·최대/최소·잠금·잘못된 입력·Undo/Redo·JSON·버전·키 복사/시간 이동·색 초안 격리, 색/알파 중간점·겹친 스톱·레이어 간 정의 격리·점선·방사형 초점·길이 0을 검사했다. 독립 수식과 Preview/출력 일치를 검사했으며 8-bit premultiplied 채널 허용 오차는 2/255다. Cargo check/fmt/test/release 및 git diff --check 통과(Moon/proto 미설치, 기존 경고 17개).
- 실제 Windows에서 Gradient Fill 추가, HEX FF8000 편집과 Undo/Redo, 색 스톱 추가·제거·Undo, End X의 0/60프레임 600→300 키, 30프레임 450 보간·Value Graph·저장·최신 빌드 재열기를 확인했다. 실제 저장본 `target/qa/gradient-native.lfe.json`의 0/15/30/45/60프레임에서 독립 색 계산 27,225개 샘플이 통과했다(`verify_gradient_native.py`). Linear→Radial 전환과 Undo, 색 선택기 안내, 색상바 이음선 수정도 실제 창에서 확인했다. 실제 작업 창 1개, 사용자 보존 원본 SHA256 불변을 확인했다.
- 남음: 드래그 가능한 Gradient Editor·중간점/스톱, Composition 끝점 손잡이, AE의 단일 Colors 속성/토폴로지 애니메이션. 현재 스톱 추가/제거는 정적 구조 변경이며 다른 프레임의 결과를 바꿀 수 있다. Gradient Stroke·Opacity 스톱·Highlight 수치의 네이티브 조작과 좁은 창의 전체 필드 검증은 남아 있다. AE 색 공간/픽셀 동등성을 주장하지 않으며 E04 전체 완료로 표시하지 않는다.

### 그룹 Skew·Skew Axis와 변형 속성 순서 — 2026-10-03

- E04/D01: Contents 그룹에 Skew(−89°–89°)·Skew Axis와 스톱워치를 추가했다. Properties와 Timeline의 그룹 속성 순서를 Anchor → Position → Scale → Skew → Skew Axis → Rotation → Opacity로 통일했다. 실제 After Effects 2026에서 이 순서와 Skew 45°/Axis 0°일 때 위쪽 변이 오른쪽으로 움직이는 방향을 확인했다. AE의 전체 수치 범위나 픽셀 동등성을 주장하지 않는다.
- 그룹은 Anchor 차감 → Scale → 축 방향 Skew → Rotation → Position 순서로 적용한다. 중첩 그룹 렌더와 기존 Bezier Pen 편집은 같은 누적 행렬/역행렬을 사용한다. 키 생성/복사/시간 이동·Graph·Undo/Redo·JSON에 공통 트랙으로 연결한다. Skew 보간 오버슈트는 ±89°로 제한하며 현재 값으로 키를 추가하거나 애니메이션을 끌 때 보이는 값을 유지한다.
- v43 Contents를 읽을 때 기존 버전 스키마를 먼저 검증하고, 모든 컴포지션의 그룹에 기본값 0인 두 속성을 추가해 v44로 변환한다. 기존 기하·페인트·키는 보존한다. v44에서 속성이 빠졌거나 새 속성을 v43으로 위장한 문서는 거부한다. 다른 모델의 구버전 파일은 이 마이그레이션으로 변경하지 않는다.
- 자동 검증: 코어 183개 + 데스크톱 238개(총 421개) 통과, 외부 미디어/장치 30개 제외. 축 0/45/90°의 독립 좌표, Anchor/Scale/Rotation 순서, 중첩 그룹과 Pen 역변환, 키 복사/시간 이동/Undo/Redo/저장, 잘못된 수치·오버슈트·버전 마이그레이션을 검사했다. 5프레임 Rectangle Preview/출력 RGBA가 독립 기하 계산과 일치했다. Cargo check/fmt/test/release 및 git diff --check 통과(Moon/proto 미설치, 기존 빌드 경고 17개).
- CLI 검증: 생성한 `target/qa/skew-generated.lfe.json`의 0/15/30/45/60프레임 출력 RGBA 155,517개 샘플이 독립 계산과 일치했다(`verify_skew_fixture.py`). 기존 실제 v43 저장본 `contents-paint-native.lfe.json`도 새 빌드로 읽어 내부/외부 15,951개와 점선/간격 600개 샘플을 다시 통과했다. 생성한 Skew 파일은 네이티브 UI 저장본이 아니다.
- 후속 네이티브 검증: 창 활성화 오류가 복구된 뒤 실제 v43 사본을 열어 Skew 45° 입력·Undo/Redo, 45/60프레임 45→30° 키와 Graph, Axis 90° 정적 입력·v44 저장을 확인했다. `target/qa/skew-native.lfe.json`은 이제 실제 편집 저장본이며 0/30/45/50/60프레임의 독립 기하/색 계산 66,059개 샘플이 통과했다(`verify_skew_native.py`). 1442px 창에서 조작했으며 새 실행 시 이전 편집기가 닫히고 작업 창 하나를 유지했다. 사용자 보존 원본 SHA256 불변. **Skew 저장본의 네이티브 재열기·Axis 스톱워치·기울어진 경로의 실제 Pen 드래그는 아직 미검증**이다.
- 다음: 남은 네이티브 검증과 페인트 Blend·경로 연산자, 트리 드래그/다중 선택과 선택 그룹으로 새 Pen 경로 삽입을 개발한다. Gradient 페인트와 Composite는 위 후속 단계에서 구현했다. E04 및 전체 백로그는 계속 진행 중이다.

### Contents 트리·다중 경로/페인트·그룹 변형 — 2026-10-03

- E04/D01: 기존 Shape의 `Create Contents Group`이 경로·Stroke·Fill을 별도 항목으로 분리한다. 기하·경로·색·불투명도·점선 키를 유지하고 한 Undo로 원래 모델을 복원한다. 새 모델은 프로젝트 v43이며 기존 파일은 명시적으로 전환할 때까지 유지한다. 레이어별 안정된 항목 ID를 사용하고 복제 시 하위 항목까지 새 ID를 부여한다. 최대 256항목/8단계 중첩을 검증한다.
- Properties에 그룹 접기, 항목 추가/이름/가시성/복제/삭제/순서 변경, 그룹 안팎 이동, 선택 항목의 수치와 스톱워치를 연결했다. 그룹 Position/Anchor/Scale/Rotation/Opacity, 경로 Width/Height/Position·기하, Fill/Stroke 수치 트랙이 공통 Timeline/Graph·키 복사·시간 이동을 사용한다. 기존 Contents Bezier 경로는 누적 그룹/레이어 좌표 변환으로 Pen 편집한다.
- [Adobe의 페인트 순서](https://helpx.adobe.com/after-effects/desktop/drawing-painting-and-paths/shapes-and-shape-attributes/shape-attributes-paint-operations-path.html)를 참고했다. 각 페인트는 같은 그룹에서 자신보다 위에 있는 경로·하위 그룹 경로를 사용하며, 먼저 나오는 페인트/그룹을 앞에 합성한다. Fill은 여러 경로를 하나의 복합 경로로 채우므로 겹친 부분의 페인트 알파를 중복 적용하지 않는다. Non-Zero/Even-Odd 규칙과 그룹 합성 후 불투명도를 지원한다. 곡선 기본 도형은 cubic 근사이다.
- 자동 검증: 코어 178개 + 데스크톱 235개(총 413개) 통과, 외부 미디어/장치 30개 제외. 전환/재정렬/부모 이동/복제/삭제의 원자성·잠금·버전·Undo/Redo·JSON·키 이동, 기존 5종 도형의 애니메이션 렌더 보존, 페인트 순서·복합 알파·Even-Odd·중첩 그룹 변형/불투명도·Preview/출력 일치를 검사했다. Cargo check/fmt/test와 git diff --check 통과(Moon/proto 미설치).
- 실제 Windows에서 기존 Star의 Contents 전환 Undo/Redo, Fill 복제·색/불투명도 편집·페인트 순서 Undo/Redo, 그룹 Position X의 30/60프레임 0→200 키와 45프레임 100 보간, U 필터·Value Graph·저장 재열기를 확인했다. 1920/1442px 창 배치, 실제 작업 창 1개와 보존 원본 SHA256 불변을 확인했다. Release build도 통과했다(기존 경고 17개).
- 실제 저장본 `target/qa/contents-native.lfe.json`과 순서 비교본의 0/15/30/45/60프레임 출력에서 독립 기하·페인트 합성 계산의 RGBA 45,567개 샘플이 일치했다(`target/qa/verify_contents_native.py`). 중첩 그룹/복합 경로는 자동 검사 범위이며 네이티브 조작을 별도 검증하지 않았다.
- 후속 연결: Fill/Stroke 색 스와치에서 공통 HEX/RGB/HSV·불투명도·최근 색·뷰어 샘플링 대화상자를 연다. 취소는 문서를 변경하지 않고 승인은 현재 프레임에서 바뀐 채널만 한 Undo로 적용한다. Stroke의 Cap/Join 선택과 마지막 Dash/Gap 추가·삭제를 연결했다. 다른 트랙은 보존하고 Undo로 제거한 점선 키를 복원하며 행 변경 시 수치·스톱워치도 갱신한다.
- 후속 검증: 코어 179개 + 데스크톱 237개(416개) 통과, 30개 제외. 색 초안/채널 격리·다른 페인트 보존·시간 보간·잠금·오래된 초안·Undo/Redo·JSON, 점선 한계/행 삭제/키 복원, 같은 Bezier 경로의 9가지 Cap/Join 조합에서 기존 Shape와 Contents의 전체 RGBA 및 Preview/출력 일치를 검사했다. Cargo check/fmt/test/release와 git diff --check 통과, 기존 경고 17개. 기본 다각형과 cubic 표현의 점선 경계 래스터 차이는 픽셀 동등성 범위에서 제외한다.
- 실제 Windows에서 Fill 색 선택기 취소/HEX 변경/Undo/Redo, Projecting Cap·Bevel Join, Stroke Width 24·Dash 10·Gap 50, Gap 스톱워치 활성화·행 제거 후 Undo·저장 재열기와 작은 창 배치를 확인했다. `target/qa/contents-paint-native.lfe.json`의 0/30/45/60프레임 출력에서 내부/외부 15,951개 및 점선/간격 600개 RGBA 샘플이 독립 계산과 일치했다(`target/qa/verify_contents_paint_native.py`). 작업 창 1개와 보존 원본 SHA256 불변을 확인했다. 이번 네이티브 검사는 Fill HEX 중심이며 Stroke 색 입력·뷰어 샘플링은 별도 조작하지 않았다.
- 그룹 Skew·Gradient 페인트·Composite는 위 후속 단계에서 구현했다. 남음: 페인트 Blend, 경로 연산자, 트리 드래그/다중 선택과 새 Pen 경로의 선택 그룹 삽입. AE의 그룹 좌표/페인트 합성 픽셀 동등성을 검증한 것은 아니며 E04 전체 완료로 표시하지 않는다.

### 기본 도형 → Bezier 경로 전환 — 2026-10-03

- E02/E03: Properties의 `Convert To Bezier Path`로 Rectangle·Rounded Rectangle·Ellipse·Polygon·Star를 현재 프레임의 닫힌 경로로 전환한다. Points·Inner Radius·Roundness 트랙은 평가된 정적 기하로 대체하고 페인트 트랙·레이어 변형·마스크·효과는 유지한다. 툴팁에서 이 동작을 설명한다. 한 Undo로 원래 도형과 키를 복원하며, 전환 후 기존 Path 스톱워치와 Pen 편집으로 경로 애니메이션을 만들 수 있다. 잠금·없는 레이어·비도형·이미 전환된 경로·잘못된 시간은 원자적으로 거부한다.
- [Adobe의 경로 전환 흐름](https://helpx.adobe.com/after-effects/desktop/animate-in-after-effects/animate-shape-paths-and-masks/animating-shape-paths-masks.html)을 참고했다. Libre Effects는 기존 레이어 좌표의 경로를 생성하고 현재 파일 형식을 사용한다. 원과 둥근 모서리는 4개의 cubic 원호로 근사한다. AE Contents의 그룹 좌표/변환이나 픽셀 동등성까지 구현한 것은 아니다.
- 검증: 코어 175개 + 데스크톱 232개(총 407개) 통과, 외부 미디어/장치 30개 제외. 5종 도형·Fill/Stroke·페인트 애니메이션의 Preview/출력, 전환/경로 키의 Undo/Redo·저장 왕복을 검사했다. Cargo check/fmt/test/release build와 git diff --check 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 30프레임의 Star를 14정점 경로로 전환하고 Undo/Redo를 확인했다. Path 애니메이션 활성화 후 60프레임의 윗꼭짓점을 드래그했으며, 드래그 Undo/Redo·45프레임 보간·저장 후 재열기·U 필터를 확인했다. 1920/1442px 창 배치와 실제 작업 창 1개, 사용자 보존 원본 SHA256 불변을 확인했다.
- 실제 저장본 `target/qa/path-conversion-native.lfe.json`의 0/30/45/60/90프레임 출력에서 독립 다각형 계산의 내부/외부 RGBA 22,155개 샘플이 일치했다(`target/qa/verify_path_conversion.py`). 전환 직후 별의 출력은 경계 48픽셀에만 알파 최대 16/255의 차이가 있었고, 적분 알파 차이는 3.004px였다. 정적 전환본의 30/90프레임은 전체 RGBA가 동일하다. 곡선 도형과 페인트 애니메이션의 네이티브 조작은 별도 미검증이다.
- 남음: Contents 그룹·다중 경로/페인트·도형 연산자, 여러 정점 선택·경로 방향/첫 정점 편집. E02/E03 전체 완료로 표시하지 않는다.

### Polygon·Star Points 애니메이션 — 2026-10-03

- E03/D01: Polygon·Star의 Points를 3–128 범위의 소수 속성으로 편집하고 스톱워치·Timeline·Graph·공통 키 편집에 연결했다. Polygon은 평가 값의 정수 부분으로 꼭짓점을 만들고 Star는 소수 부분에 따라 부분 꼭짓점이 자란다. 기존 정수 도형 좌표와 선의 시작점은 유지한다. Bezier 경로나 다른 종류의 도형에는 Points 트랙을 붙여넣을 수 없다.
- 정수 기본 필드의 이전 JSON 형식은 유지하고 Points 트랙을 사용한 파일만 v42로 올린다. 애니메이션 해제는 현재 소수 값을 보존한다. 코어 검사는 정수 형상 보존·소수 경계·면적 연속성·키 이동/복사·Hold·Undo/Redo·저장 왕복·잘못된 값/대상/버전의 원자적 거부를 포함한다. Preview/출력 일치와 독립 면적식도 검사했다.
- 검증: 코어 173개 + 데스크톱 231개(총 404개) 통과, 외부 미디어/장치 30개 제외. Cargo check/fmt/test/release build와 git diff --check 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 Star Points 5.25→8.75를 0/60프레임 키로 입력했다. Undo/Redo, 30프레임 Points 7의 별, U 필터·Value Graph, 실제 저장본 `target/qa/points-native.lfe.json` 재열기를 확인했다. 0/15/30/45/60프레임 PNG의 19,358개 내부/외부 RGBA 샘플이 독립 기하 계산과 일치하며 적분 알파 면적 오차는 3.36px 미만이다(`target/qa/verify_points_native.py`). 1920/1442px 창 배치를 확인했다. 실제 작업 창은 1개이며 사용자 보존 원본 SHA256은 유지됐다.
- 남음: Contents 그룹·다중 Fill/Stroke·Gradient 페인트·연산자, 독립 경로 회전/반지름·안팎 Roundness와 AE 픽셀 동등성. Polygon의 네이티브 입력 조작은 별도 미검증이며 모델·출력 자동 검사로 확인했다. E03 전체 완료로 표시하지 않는다.

### 도형 곡선의 한계값 유지 — 2026-10-03

- D01/E03: 유효한 Bezier 곡선의 중간 값이 속성 한계를 넘을 때, 키 추가·애니메이션 해제가 `Invalid shape property or keyframe`으로 거부되는 문제를 회귀 테스트로 재현하고 수정했다. 이 두 조작은 현재 렌더에서 평가한 한계값을 저장하며, 직접 입력한 범위 밖 수치는 계속 거부한다. 기존 키/핸들·다른 속성은 보존하고 한 번의 Undo/Redo로 복원한다. Timeline 숫자 필드와 잠긴 행도 공통 속성 평가 값을 사용한다. Graph 곡선은 조절 가능한 원래 보간 곡선을 유지한다.
- 코어 회귀 검사는 Width·Miter·Offset·Fill/Stroke Opacity·RGB의 상·하한 오버슈트, 두 조작, 이전 키 보존, Undo/Redo·JSON과 잘못된 시간의 원자적 거부를 포함한다. 실제 Windows에서는 Fill Red가 286.875로 오버슈트하는 프레임에 RGB 키를 추가하여 255로 저장했고, 키 추가 및 스톱워치 해제 각각 Undo/Redo를 확인했다.
- 실제 저장본 `target/qa/shape-bounds-native.lfe.json`의 Fill RGB는 255/159.5/191.5로 고정된다. 0/15/30/45/60프레임 출력 6,300개 샘플이 기대 색/알파와 2단계 이내로 일치하며, 30프레임 전체 RGBA는 해제 전후 정확히 동일하다(`target/qa/verify_shape_bounds_native.py`). 모델로 만든 원래 곡선 사본과 실제 UI에서 편집·저장한 사본을 구분해 보존했다.
- 최종 검증: 코어 170개 + 데스크톱 230개(총 400개) 통과, 외부 미디어/장치 30개 제외. Cargo check/fmt/test/release build와 git diff --check 통과(Moon/proto 미설치), 기존 경고 17개. 최종 릴리스에서 원래 곡선의 Timeline R 표시 255.00을 확인했고, 실제 편집 저장본 재열기 후 Fill FFA0C0·꺼진 색상 스톱워치·미리보기 유지를 확인했다. 실제 작업 창 1개와 사용자 보존 원본 SHA256 불변을 확인했다.

### 도형 Fill·Stroke 색상 애니메이션 — 2026-10-03

- E03/D01: Properties의 Fill/Stroke 색상 스톱워치가 RGB 세 채널을 함께 켜고 끈다. 다른 프레임의 HEX 입력·색 선택기는 RGB 키를 한 Undo로 편집한다. Timeline의 Fill Color/Stroke Color 그룹과 R/G/B별 Graph 선택, 공통 키 복사·시간 편집·보간을 연결했다. 일부 채널만 붙여넣은 경우 색상 편집 시 나머지 채널의 기본색을 최초 색상 키 시점에 보존한다.
- 도형 색 선택기의 알파는 해당 Fill/Stroke 불투명도를 편집하며 레이어 Transform Opacity는 유지한다. RGB와 알파의 동시 변경도 한 Undo이고, 확정 전 임시 편집은 문서를 바꾸지 않는다. 프로젝트 v41은 색상 트랙을 사용할 때만 필요하며 이전 정적 색상은 유지한다. 보간은 0–255 인코딩 RGB 기준이며 표시/출력에서 8비트로 제한·반올림한다.
- 검증: 코어 169개 + 데스크톱 230개(총 399개) 통과, 외부 미디어/장치 30개 제외. 부분 채널 붙여넣기, 색상/알파 Undo/Redo, 입력 원자성·잠금·버전·JSON·키 이동, Preview/출력 RGBA를 검사했다. Cargo check/fmt/test/release build와 git diff --check 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 0/60프레임 Fill FFFFFF→204080, Stroke FFFFFF→C08020을 입력했다. Fill 선택기의 알파 40% 동시 편집과 한 번의 Undo/Redo, 30프레임 Fill 90A0C0/70%, Stroke E0C090/80%, U 필터의 색상 그룹과 Green Value Graph 159.5를 확인했다. 실제 저장 v41 `target/qa/shape-color-native.lfe.json`을 재열어 값과 미리보기를 확인했다. CLI PNG 0/15/30/45/60프레임에서 채움·선·겹침·외부 9,100개 RGBA 샘플이 독립 계산과 2단계 이내로 일치했다(`target/qa/verify_shape_color_native.py`). 1920/1442px 창의 타임라인 그룹·Properties 스크롤 표시를 확인했다.
- 남음: Points 애니메이션, Contents 그룹·다중 Fill/Stroke, Gradient Fill/Stroke와 도형 연산자. RGB 수치 보간은 구현했지만 AE 색 공간/픽셀 동등성을 뜻하지 않으며 E03 전체 완료로 표시하지 않는다.

### 도형 Fill·Stroke 불투명도 — 2026-10-03

- E03: Properties에 Fill opacity와 Stroke opacity를 별도로 추가했다. 각각 0–100%와 스톱워치를 제공하고 기존 Contents · Shape/Timeline/Graph, 키 편집, Undo/Redo에 연결한다. [Adobe Shape paint 속성](https://helpx.adobe.com/in/after-effects/desktop/drawing-painting-and-paths/shapes-and-shape-attributes/shape-attributes-paint-operations-path.html)을 참고했다. Fill/Stroke는 겹치는 영역에서 합성한 뒤 레이어 Transform Opacity를 적용한다. Fill 비활성·Stroke Width 0은 기존처럼 해당 페인트를 숨긴다.
- 이전 파일은 두 값 모두 100%이며 기본 필드는 생략한다. 비기본 정적 값 또는 새 트랙을 사용하면 프로젝트 v40으로 올린다. 구버전으로 위장한 새 데이터, 범위 밖/비유한 값, 잠긴 레이어의 편집은 거부한다. 애니메이션 해제는 현재 평가 값을 유지하며 기존 색상·경로·폭·다른 트랙은 보존한다.
- 검증: 코어 166개 + 데스크톱 229개(총 395개) 통과, 외부 미디어/장치 30개 제외. 독립 보간·정적 값/트랙 버전 검증·이전 파일 기본값·입력 원자성·Undo/Redo·JSON·애니메이션 해제·Fill/Stroke 겹침과 레이어 Opacity 50% 결합·Preview/출력 픽셀을 검사했다. Cargo check/fmt/test/release build와 git diff --check 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 0/60프레임 Fill 100→20%, Stroke 100→60% 키 입력, Stroke Undo/Redo, 30프레임 60%/80%, U 필터와 Fill Value Graph를 확인했다. 실제 저장본 `target/qa/paint-opacity-native.lfe.json`을 재열어 값/미리보기 유지를 확인했다. CLI PNG 0/15/30/45/60프레임에서 9,100개 채움·선·겹침·외부 샘플이 독립 계산과 알파 1단계 이내로 일치했다(`target/qa/verify_paint_opacity_native.py`). 1920/1442px 창의 Properties 배치, 실제 작업 창 1개를 확인했다. 열려 있는 AE 첫 화면은 읽기만 했고 기존 왼쪽 Project/중앙 Composition/하단 Timeline/오른쪽 패널 배치를 유지했다.
- 남음: Fill/Stroke 색상·Points 애니메이션, 다중 Fill/Stroke·Contents 그룹, Gradient Fill/Stroke와 도형 연산자. 실제 AE와 페인트 합성의 수치/픽셀 동등성은 별도 검증 과제이며 E03 전체 완료로 표시하지 않는다.

### 점선 길이·간격 애니메이션 — 2026-10-03

- 후속 네이티브 검증(2026-10-03): 창 입력이 복구된 뒤 v39에서 Dash 80→120, Gap 80→60, Width 32→64를 0/60프레임 키로 입력했다. Width Undo/Redo, 30프레임의 100/70/48과 Offset 100, U 필터의 Contents · Shape, Dash 1 Value Graph를 확인했다. 실제 저장본 `target/qa/dash-animation-native.lfe.json`을 재열어 값과 미리보기 유지를 확인했다. CLI 0/15/30/45/60프레임 PNG의 선 내부·외부·점선 간격 15,922개 샘플이 독립 계산과 일치했다(`target/qa/verify_dash_animation_native.py`). 1442px 창에서 조작했고 작업 창 1개 및 사용자 보존 원본 SHA256 불변을 확인했다. 아래의 입력 차단 기록은 당시 상황이며 현재는 해소됐다. Dash/Gap 제거·재추가의 네이티브 조작 및 1920px 애니메이션 배치 비교는 별도 미검증이다.

- E03/D02: 최대 16개의 개별 Dash/Gap 행에 스톱워치를 연결했다. 행의 값은 현재 프레임의 트랙을 평가하며 공통 Timeline/Graph·키 편집·Undo/Redo를 사용한다. 마지막 행 제거는 그 행의 트랙/키도 같은 변경으로 제거한다. Undo는 모두 복원하고 다시 추가한 행은 10px·키 없음으로 시작한다. 앞선 행과 Cap/Join 편집의 기존 트랙은 유지한다.
- 점선 트랙은 프로젝트 v39의 `DashLength0`…`DashLength15` 주소를 사용한다. 이전 v38의 스칼라 주소 직렬화는 유지하며 잘못된 주소, 존재하지 않는 행의 편집/붙여넣기, 행 없이 남은 트랙, v38 이하의 점선 트랙을 거부한다. 홀수 길이 목록 반복·전부 0인 실선·Round cap의 길이 0 점도 매 프레임 평가 결과에 적용된다.
- 검증: 코어 164개 + 데스크톱 228개(총 392개) 통과, 외부 미디어/장치 30개 제외. 주소 왕복/구버전 호환, 보간, 항목 제거/Undo/Redo/재추가, 기존 트랙 보존, 잘못된 값/주소/붙여넣기의 원자적 거부, Preview/출력 픽셀을 검사했다. Cargo check/fmt/test와 릴리스 컴파일 및 git diff --check 통과(Moon/proto 미설치). 실행 중인 이전 바이너리를 덮어쓰지 않도록 `cargo rustc --release -p libre-effects-desktop --bin libre-effects -- -o …/target/qa/libre-effects-dash-animation.exe`로 빌드했다. 코드의 기존 경고 17개 외 출력 경로 옵션 안내 2개가 있다.
- 생성한 모델 QA의 0/15/30/45/60프레임 CLI PNG에서 Dash 40→100, Gap 100→60의 445개 점선/투명 간격 샘플이 독립 계산과 일치했다(`target/qa/verify_dash_animation_model.py`). UI로 저장한 파일이 아니며 네이티브 조작 검증을 대신하지 않는다. 기본 `target/release/libre-effects.exe`도 새 바이너리와 같은 SHA256으로 갱신했다. 이전 실행 이미지는 `libre-effects-v38-running.exe`에 보존했고 기존 QA 창 PID 43264의 응답과 미저장 프로젝트가 유지됨을 확인했다. 다음 실행부터 v39가 적용된다.
- 실제 UI 검증은 남아 있다. 기존 v38 QA 창의 화면 캡처는 복구됐으나 입력 활성화는 같은 `failed to activate captured window` 오류로 실패했다. 60프레임 Offset 180 입력이 아직 편집 필드에 남아 있으므로 입력 확정·Undo/Redo·저장/재열기를 먼저 마친 뒤 새 v39 빌드의 Dash/Gap 조작·Graph·좁은 창 배치를 확인해야 한다. 사용자 보존 원본 SHA256은 변하지 않았다. 색상·Points 애니메이션과 Contents 그룹/연산자 등 전체 E03 잔여 범위도 유지한다.

### 도형 스칼라 속성 애니메이션 — 2026-10-03

- 후속 네이티브 검증(2026-10-03): Offset 180 입력 확정, Undo/Redo, 30프레임 값 100, Timeline/Graph 선택 및 60프레임 키 F9→Undo를 확인했다. 실제 저장 v38 `target/qa/shape-animation-native.lfe.json`을 v39에서 재열어 값/미리보기 유지와 하위 호환을 확인했다. CLI 5프레임의 Offset 20/60/100/140/180 점선 샘플 448개가 독립 계산과 일치했다(`target/qa/verify_shape_animation_native.py`). Width 실제 입력/저장/재열기/출력은 위 Dash/Gap 후속 기록에 포함된다. Roundness·Inner Radius·Miter Limit 각각의 네이티브 조작은 미검증이다. 아래 입력 차단 기록은 현재 해소됐다.

- E03/D02: Stroke Width, Roundness, Inner Radius %, Miter Limit, Dash Offset에 Properties 스톱워치를 연결했다. 켠 뒤 재생 헤드를 이동해 값을 바꾸면 해당 프레임에 키를 만든다. 처음 편집한 속성은 Timeline → Contents · Shape에 나타나며 공통 키 복사/붙여넣기·이동·삭제·시간/값 배율·보간·Graph에 연결된다. 스톱워치를 끄면 현재 프레임의 평가 값을 유지한다. 렌더 시 범위를 벗어나는 보간 오버슈트는 속성 한계로 제한한다.
- 선택적 `Shape.parameters`는 사용한 속성만 저장하고 기존 정적 값은 해당 트랙이 없는 경우의 기본값으로 유지한다. 프로젝트 v38을 사용하며 v37 이하의 기존 도형 모습은 그대로다. 레이어 이동/복사 등 공통 시간 작업에도 도형 트랙을 포함했다. 스타일의 Cap/Join 변경은 애니메이션 트랙을 보존한다.
- 검증: 코어 163개 + 데스크톱 226개(총 389개) 통과, 외부 미디어/장치 30개 제외. 다섯 속성의 보간·Undo/Redo·JSON 왕복·이전 버전 거부, 정적 경로/변형 보존, Hold·키 복사/시간 이동/배율/삭제·애니메이션 해제 값 유지·잠금/잘못된 값의 원자적 거부를 검사했다. 5프레임의 선 두께/점선 Offset 픽셀을 독립 계산하고 Preview/출력 일치를 검사했다. Cargo check/fmt/test/release build 및 git diff --check 통과(Moon/proto 미설치), 기존 릴리스 경고 17개.
- 실제 Windows에서 v37 QA 사본을 열어 Offset 스톱워치 활성화, Timeline의 Contents · Shape/키 생성, 60프레임 이동과 값 180 입력까지 확인했다. 입력 확정 확인 시 컴퓨터 제어 도구의 `failed to activate captured window` 오류가 복구 시도에서도 반복됐다. 따라서 **이번 애니메이션의 실제 UI 입력 확정·Undo/Redo·저장/재열기·Graph 조작은 미완 검증**이다. 자동 테스트와 구분하며 이후 같은 QA 사본에서 이어서 확인한다. 이번 턴에 실제 AE 초기 패널 배치를 다시 읽어 대조했고 문서를 수정하지 않았다.
- 별도 생성한 모델 QA 파일의 0/15/30/45/60프레임 릴리스 CLI PNG에서 Width 16→64, Offset 20→180의 17,472개 내부/외부/점선 픽셀이 독립 계산과 일치했다(`target/qa/verify_shape_animation_model.py`). 이 파일은 UI 저장 결과가 아니다. 기존 실제 v37 QA의 264개 샘플도 새 릴리스에서 통과했다. 실제 편집기 1개와 사용자 보존 원본 SHA256 불변을 확인했다.
- 남음: 개별 Dash/Gap 길이·색상·Points 애니메이션, 다중 Fill/Stroke·Contents 그룹과 연산자. 전체 E03 완료로 표시하지 않는다.

### 도형 선 끝·모서리·점선 — 2026-10-03

- E03: 기존 Properties의 Stroke 아래에 Butt/Round/Projecting 끝 모양, Miter/Round/Bevel 모서리, Miter Limit, Dash Offset, Dash/Gap 길이와 추가/제거 버튼을 연결했다. 기본 도형과 Pen 경로에 공통 적용하며 패널 배치는 유지한다. 조작 항목은 [Adobe Shape 속성 설명](https://helpx.adobe.com/in/after-effects/desktop/drawing-painting-and-paths/shapes-and-shape-attributes/shape-attributes-paint-operations-path.html)을 참고했다. 홀수 길이 목록 반복과 전부 0인 목록의 실선 처리는 [SVG 페인팅 규칙](https://www.w3.org/TR/SVG2/painting.html)을 따른다.
- Miter Limit 1–1024, Dash/Gap 각각 0–8192px 최대 16개, Offset ±32768px를 지원한다. 음수 길이·비유한 값·잠긴 레이어·사라진 항목은 변경을 거부한다. 메뉴는 위/아래·Enter·Escape와 포커스/창 전환 닫기를 지원한다. 메뉴 행을 클릭할 때 포커스가 먼저 빠져 선택이 취소되던 문제를 실제 창에서 찾아 수정했다. 편집은 각각 한 Undo이며 경로·키·변형은 유지한다. 비기본 스타일은 프로젝트 v37을 사용하고 기존 파일의 Butt/Round/실선 결과를 보존한다.
- 검증: 코어 160개 + 데스크톱 225개(총 385개) 통과, 외부 미디어/장치 30개 제외. 끝 모양·모서리 기하, Miter 제한, 홀수 점선 주기·Offset·길이 0의 둥근 점·전부 0인 실선, 입력 한계와 원자성, Undo/Redo·JSON·반투명 Preview/출력 일치를 검사했다. Cargo check/fmt/test/release build 및 git diff --check 통과(Moon/proto 미설치), 기존 릴리스 경고 17개.
- 실제 Windows에서 Projecting 클릭, 키보드 Miter 선택, Dash 80/Gap 80/Offset 20 입력, Offset Undo/Redo와 저장·재열기를 확인했다. 실제 저장 파일 `target/qa/shape-stroke-native.lfe.json`의 0/30/60프레임 960×540 PNG에서 점선·빈 간격 264개 샘플이 독립 계산과 일치했고 프레임 사이 정확한 40px 이동도 확인했다(`target/qa/verify_shape_stroke_native.py`). 1920/1442 폭의 속성·메뉴 배치, 실제 작업 창 1개, 사용자 보존 원본 SHA256 불변을 확인했다.
- 이번 선 스타일은 정적 값이다. Stroke 속성 애니메이션, 다중 Fill/Stroke·Contents 그룹, Gradient Fill/Stroke, 연산자는 남아 있다. E03 전체 완료나 AE 픽셀 동등성으로 표시하지 않는다.

### Composition 그라디언트 끝점 편집 — 2026-10-03

- G05: Linear/Radial Gradient의 Effect Controls → Edit gradient in Composition에서 시작점·끝점을 직접 편집한다. 선택한 효과 인스턴스의 레이어 좌표를 부모 변형·회전·반전·뷰 확대/이동에 맞춰 표시하고 기존 효과 트랙으로 저장한다. 시작점은 십자, 끝점은 사각형이며 연결선을 표시한다. Shift-drag는 레이어 축 제한, Alt-drag는 양 끝점 이동이다. Composition 포커스에서 Tab으로 끝점을 선택하고 방향키 1px·Shift 10px·Alt 양 끝점 이동을 지원한다.
- 드래그는 임시 프로젝트로 미리 보고 놓을 때 한 Undo로 확정한다. 실제로 바뀐 좌표만 갱신하여 고정 축에 불필요한 키를 만들지 않는다. Escape는 드래그 취소, 다음 Escape는 컨트롤 닫기다. 문서·시간·선택·도구·뷰 변경과 포커스 이탈/창 비활성화는 미확정 드래그를 취소한다. 잠금·우회·비활성·역변환 불가 레이어에는 편집 손잡이를 표시하지 않는다. 기존 ±32768 좌표 범위 밖 편집은 거부하고 컨트롤 자체는 저장/출력에서 제외한다. 프로젝트 스키마 변경은 없다.
- 검증: 코어 158개 + 데스크톱 221개(총 379개) 통과, 외부 미디어/장치 30개 제외. 부모 회전/반전·8px 히트 범위·초기 포인터 오프셋·반복 이동의 비누적성·Shift/Alt·키/핸들 보존·원자적 Undo/Redo·JSON·상태 변경 취소·범위 제한·Linear/Radial의 독립 색상 계산·Preview/출력 일치를 검사했다. Cargo check/fmt/test/release build 및 git diff --check 통과(Moon/proto 미설치), 기존 릴리스 경고 17개.
- 실제 Windows에서 30프레임 끝점 (700,400)을 드래그해 (551.034488,298.206900)으로 변경하고 한 번의 Undo/Redo를 확인했다. Tab→Shift+Right로 시작 X를 110으로, Alt+Shift+Down으로 시작 Y를 110·끝 Y를 308.206900으로 바꿨다. 저장/재열기 후 값과 미리보기가 유지됐다. `target/qa/gradient-points-native.lfe.json`의 0/15/30/45/60프레임 960×540 PNG에서 각 198개, 총 990개 색상 샘플이 독립 선형 그라디언트 계산과 정확히 일치했으며 알파 경계와 레이어 변형 불변도 확인했다. 1920/1442 폭의 패널·손잡이 표시, 실제 작업 창 1개, 사용자 보존 원본 SHA256 불변을 확인했다.
- 실제 UI의 Shift/Alt를 누른 포인터 드래그 및 드래그 도중 Escape/창 전환은 입력 도구 제약으로 미검증이다. 관련 계산/취소 상태는 자동 테스트 범위로 구분한다. 그라디언트 Scatter/디더링, 캔버스에서 색상 편집·스냅과 다른 효과의 공간 컨트롤은 남아 있다. G05 전체 완료나 AE 픽셀 동등성으로 표시하지 않는다.

### 타임라인 보간 모양·키프레임 메뉴 — 2026-10-03

- D02/B05: 타임라인 키의 왼쪽/오른쪽 절반으로 incoming/outgoing 상태를 구분한다. Linear는 다이아몬드, Hold는 사각형, Auto Bezier는 원형, 수동/Continuous Bezier는 안쪽으로 들어간 모양이다. 인접 키가 없거나 이전 Hold가 incoming을 억제하면 어둡게 표시한다. 결합 행의 채널 상태가 다르면 표시가 있는 다이아몬드와 채널별 툴팁을 제공한다. 선택 윤곽은 유지하며 프로젝트 데이터는 변경하지 않는다. [Adobe 보간 표시 설명](https://helpx.adobe.com/after-effects/desktop/animate-in-after-effects/animation-keyframes/keyframe-interpolation.html)을 참고했다.
- 키 우클릭 또는 타임라인 포커스의 Shift+F10으로 Easy Ease/In/Out, Auto/Continuous/Independent Bezier, Linear/Hold outgoing segment, Graph 열기, 삭제를 실행한다. 선택된 키의 우클릭은 기존 그룹을 유지한다. 위/아래·Home/End·Enter·Escape로 조작하며 비활성 항목을 건너뛴다. 문서/선택 변경·포커스 이탈·창 비활성화·바깥 클릭은 메뉴를 닫는다. 편집은 전체 선택을 검증한 한 Undo이며 잠금·경로 키의 미지원 명령을 비활성화한다.
- 검증: 코어 158개 + 데스크톱 215개(총 373개) 통과, 외부 미디어/장치 30개 제외. 모양 분류·결합 행·끝점·Hold·선택 유지·메뉴 무효화·명령 원자성·Undo/Redo·JSON·Preview/출력 회귀를 검사했다. Cargo check/fmt/test/release build 및 git diff --check 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 우클릭 Auto Bezier/Hold 적용, 한 번의 Undo/Redo, Shift+F10→키보드 Graph 열기와 포커스, 저장·재열기를 확인했다. 최종 빌드에서 8개 키 선택 유지·End 단일 강조·Escape 취소, 1920/1442 폭의 메뉴 배치를 확인했다. QA 사본 `target/qa/key-menu-native.lfe.json`의 30/45/60/67/75/83/90프레임 PNG 중심은 독립 계산 720/1095/1320/1119.644444/720/720/1320px와 출력 해상도 1px 이내로 일치했다. 재열기 후 83프레임 Position X 720 미리보기를 확인했다. 실제 작업 창 1개와 사용자 보존 원본 SHA256 불변을 확인했다.
- 남음: AE 공간 보간·다차원 속도, 경로 Ease In/Out, 배율별 접근성/화면 회귀. Graph는 현재 스칼라 채널을 편집하며 메뉴 Linear/Hold는 outgoing 구간을 바꾼다. AE와 픽셀 단위 UI 또는 수치 동등성은 검증하지 않았다. D02/B05 전체 완료로 표시하지 않는다.

### 타임라인·그래프 Easy Ease 단축키 — 2026-10-03

- B05/D02의 F9(Easy Ease), Shift+F9(Ease In), Ctrl+Shift+F9(Ease Out)을 구분했다. 그래프가 모든 F9 조합을 양방향 Ease로 처리하던 문제를 수정하고, 타임라인에도 같은 명령을 연결했다. 그래프는 현재 표시한 스칼라 채널, 타임라인은 레이어/속성에 걸친 선택 스칼라 키에 한 번의 Undo로 적용한다. [Adobe 공식 단축키 표](https://helpx.adobe.com/fi/after-effects/desktop/get-started/keyboard-shortcuts/keyboard-shortcuts-reference.html)를 기준으로 하며 내부 수치 보간의 AE 동등성을 뜻하지 않는다.
- 패널 자체에 포커스가 있고 드래그 중이 아닐 때 적용한다. 키 반복과 Ctrl+F9/Alt 조합은 무시하고 입력 필드는 자체 키 입력을 유지한다. 빈 선택·해당 방향의 인접 구간이 없는 키는 편집하지 않는다. 잠금·사라진 키·경로 포즈가 섞이면 전체 작업을 거부하고 상태 메시지로 알린다. 그래프 버튼 툴팁·도움말·README도 실제 동작과 맞췄다.
- 검증: 코어 158개 + 데스크톱 206개(총 364개) 통과, 외부 미디어/장치 30개 제외. 여러 레이어·채널의 방향별 핸들, 반대쪽 곡선/시간/값 유지, 원자적 거부, Undo/Redo·JSON·Preview/출력 일치를 검사했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 그래프 Shift+F9는 60프레임 incoming만, Ctrl+Shift+F9는 outgoing만 0 속도·1/3 영향도로 바꿨다. 원상복원 후 타임라인 키 선택→F9로 양방향 적용, 한 번의 Undo/Redo, 저장·재열기를 확인했다. `target/qa/ease-in-native.lfe.json`, `ease-out-native.lfe.json`, `ease-both-native.lfe.json`의 각 6프레임(총 18개) CLI PNG 중심이 독립 Hermite 계산과 출력 해상도 1px 이내로 일치했다. 최종 사본 재열기 후 67프레임 Position X 1119.64와 미리보기를 확인했다.
- 경로 포즈의 Ease In/Out, AE 공간 속도·다차원 보간, 타임라인 키 모양의 보간별 구분은 후속 범위다. D02/B05 전체 완료로 표시하지 않는다.

### 선택 변형 스냅·숫자 입력 포커스 — 2026-10-03

- D03 Value Graph 변형 박스의 움직이는 시간/값 경계에 8 논리 픽셀 스냅을 연결했다. 고정 경계 또는 Alt 중앙 기준점을 유지하며 시간 후보는 선택 키 전체의 반올림 충돌·미선택 키 충돌·컴포지션 범위를 검사한다. 값은 현재 채널의 미선택 키 값에 맞추고 주황색 안내선을 표시한다. Ctrl은 Snap 설정을 반전한다. 박스에서 Alt는 중앙 기준 조절이며, 일반 키 이동의 Alt 스냅 해제와 구분한다.
- 그래프 숫자 입력의 Enter 확정·Escape 취소 후 포커스를 그래프로 돌려준다. 추가 클릭 없이 Undo와 그래프 단축키를 사용하고 다음 Escape로 팝업을 닫을 수 있다. 다른 패널의 기존 입력 종료 동작은 유지한다.
- 검증: 코어 158개 + 데스크톱 203개(총 361개) 통과, 외부 미디어/장치 30개 제외. 스냅 화면 거리·축 제한·고정/중앙 기준·반사·시간 후보 충돌·미리보기/확정·Undo·저장 왕복·출력 일치를 검사했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 같은 모서리 드래그를 비교했다. Snap 해제 시 선택 키가 30/59/74프레임·720/1300.5/720, 활성 시 30/60/75프레임·720/1320/720으로 변경되며 미선택 90프레임·1320은 유지됐다. 한 번의 Undo/Redo와 저장 후 재열기, 60프레임 Position X 1320 미리보기를 확인했다. QA 사본 `target/qa/graph-box-snap-native.lfe.json`의 30/45/60/67/75/83/90프레임 CLI 출력 중심은 독립 선형 계산 720/1020/1320/1040/720/1040/1320px와 일치했다.
- 실제 입력에서 1320→1200→Enter 후 마우스 조작 없이 Ctrl+Z로 1320 복원, Escape로 팝업 닫기를 확인했다. 1400 임시 입력→Escape 취소→다음 Escape 닫기도 확인했다. 실제 작업 창 1개와 사용자 보존 원본 SHA256 불변을 확인했다.
- Ctrl/Alt를 누른 채 드래그하는 네이티브 조합은 입력 도구 제약으로 미검증이며 관련 계산은 자동 테스트로 확인했다. Speed Graph 변형 박스·사용자 기준점·기울이기·시간 역전·다중 채널은 남아 있으며 D03 전체 완료로 표시하지 않는다.

### Value Graph 선택 변형 박스 — 2026-10-03

- D03의 선택 변형 박스를 그래프 하단 사각형(Gravity Icons) 버튼으로 켠다. 둘 이상 선택한 스칼라 Value Graph 키의 좌우 핸들은 시간, 위아래는 값, 모서리는 두 축을 반대쪽 경계 기준으로 조절한다. Alt는 선택 중앙을 기준으로 조절한다. 같은 값의 키에는 시간 핸들만 표시한다. 키 자체를 드래그하는 기존 그룹 이동도 유지한다.
- 드래그 시작 화면과 포인터 차이를 고정하고 코어와 공유하는 계산으로 곡선을 미리 표시한다. 확정 전에는 문서를 바꾸지 않으며 놓을 때 한 번의 Undo로 저장한다. 그래프 밖 릴리스 좌표도 처리한다. Escape·포커스 이탈·문서/채널 변경은 미확정 편집을 취소한다. 시간 역전/범위 이탈/반올림 충돌은 거부하고 해당 미리보기 박스를 빨간색으로 표시한다. 속성별 값 범위는 확정 시 검사한다.
- 검증: 코어 158개 + 데스크톱 199개(총 357개) 통과, 외부 미디어/장치 30개 제외. 반대 경계/중앙 기준, 축 제한·초기 포인터 오프셋·절대 드래그, 충돌/잠금/Speed 모드, 값 반사·평탄 선택, 미리보기와 확정 곡선 일치, Undo/Redo·JSON·Preview/출력 픽셀을 검사했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 오른쪽 핸들로 30/50/60프레임을 30/60/75로 늘리고 값 720/1020/720 유지를 확인했다. 이어 모서리로 30/67/85프레임·720/958.8/720으로 변경하고 한 번의 Undo/Redo 및 저장을 확인했다. QA 사본은 `target/qa/graph-box-time-native.lfe.json`, `target/qa/graph-box-native.lfe.json`이다. 최종 사본의 30/45/67/76/85프레임 CLI 출력 중심은 독립 선형 계산 720/816.810811/958.8/839.4/720px와 출력 해상도 1px 이내로 일치했다.
- 최종 빌드에서 사본을 재열어 67프레임 Position X 958.80과 미리보기를 확인했다. 1920/1442 폭에서 변형 박스를 확인하고, 1442 폭에서 Keyframe 버튼이 잘리지 않도록 하단 간격을 보정했다. 실제 작업 창 1개와 사용자 보존 원본 SHA256 불변을 확인했다.
- 남음: Speed Graph 변형 박스·변형 스냅·사용자 기준점 이동·기울이기·시간 역전·다중 채널. Alt를 누른 채 드래그, 드래그 중 Escape, 바깥 릴리스의 네이티브 조작은 도구 입력 제약 때문에 별도 미검증이며 관련 계산과 취소 모델은 자동 테스트로 확인했다. D03 전체 완료로 표시하지 않는다.

### 선택 키의 시간·값 배율 — 2026-10-03

- Graph → Keyframe...에서 현재 스칼라 채널의 키를 둘 이상 선택하면 Time % / Value %를 조절한다. 시간은 첫 선택 키, 값은 선택 키의 최솟값을 기준으로 확대·축소한다. 시간 배율은 양수이며 값 배율은 0과 음수도 허용한다. Speed Graph에서도 Value %는 속성이 가진 값을 변경한다.
- 시간은 정수 프레임으로 반올림한다. 키 충돌·컴포지션 이탈·속성 범위 초과·잠긴 레이어는 전체 변경을 거부한다. 수동 속도는 값 배율/시간 배율로 보정하고 영향도·보간 모드를 보존한다. 한 번의 적용은 한 Undo이며 적용 후 선택 키와 활성 키가 새 시간으로 이동한다. 기존 프로젝트 스키마를 사용한다.
- 검증: 코어 158개 + 데스크톱 195개(총 353개) 통과, 외부 미디어/장치 30개 제외. 자동/연속/독립 보간의 곡선, 충돌의 원자성, 잠금·범위·반사·동일 배율, Undo/Redo·JSON 복원·미리보기/출력 일치를 검사했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 30/50/60프레임의 720/1020/720 키를 Time 150%, Value 200%로 바꾸어 30/60/75프레임의 720/1320/720으로 저장했다. Undo 두 번/Redo 두 번, 재열기 후 60프레임 미리보기 1320을 확인했다. QA 사본 `target/qa/key-scale-native.lfe.json`의 30/45/60/67/75프레임 CLI 출력 중심은 독립 계산 720/1020/1320/1040/720px와 일치했다. 작업 창 1개와 사용자 보존 원본 SHA256 불변을 확인했다.
- 남음: 포인터 변형 박스·사용자 기준점·시간 역전·다중 채널 편집. 정수 반올림 또는 일부 키만 선택한 경우 인접 곡선 전체의 정확한 상사 변환은 보장하지 않는다. 숫자 입력 종료 후 팝업 Escape 포커스 복귀도 보정할 예정이다. D03 전체 완료로 표시하지 않는다.

### 그래프 Space 임시 손 도구 — 2026-10-03

- 그래프 자체에 키보드 포커스가 있을 때 Space를 누른 채 드래그하면 선택한 툴바 도구를 유지하면서 Hand로 이동한다. 드래그에 사용한 Space 해제는 재생을 시작하지 않으며, 드래그 없이 눌렀다 놓으면 키 해제 시 Preview를 전환한다. 기존 H/중간 버튼의 이동 계산과 높이·시간 경계·문서 불변 규칙을 공유한다.
- 키 반복은 추가 재생을 만들지 않는다. Escape, 포커스 이탈, 창 비활성화는 임시 손 도구 상태를 해제하고 미확정 그래프 이동·확대를 취소한다. 기존 키 드래그 도중 누른 Space나 다른 단축키와 함께 쓴 Space도 해제 시 재생하지 않는다. 숫자 입력 필드는 자체 입력을 유지한다. 현재 범위는 그래프 포커스이며 Composition 등 다른 패널의 임시 Space 도구 통합은 남아 있다.
- 검증: 코어 154개 + 데스크톱 194개(총 348개) 통과, 외부 미디어/장치 30개 제외. Space 탭/드래그/키 반복/취소/포커스 복귀 상태 전이를 검사했고 기존 패닝·확대의 Undo/Redo·저장/출력 회귀도 통과했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치), 기존 릴리스 경고 17개.
- 실제 Windows에서 그래프 Space 탭 재생·정지와 해제 후 키 선택 복귀를 확인했다. 확대 QA 사본 재열기와 실제 저장 파일의 출력 4프레임도 확인했다. Space를 누른 채 포인터 드래그, 드래그 도중 Escape 및 포커스 이탈은 자동 입력 도구의 조합 입력 제약으로 네이티브 미검증이다. 이 조합들을 실제 검증 완료로 표시하지 않는다. 작업 창 1개와 사용자 보존 원본 SHA256 불변을 확인했다.

### 그래프 확대 도구 — 2026-10-03

- D03 그래프에 기존 Z 확대 도구를 연결했다. 클릭은 2배 확대, Alt-click은 1/2 축소하며 시작 포인터의 시간·값을 기준으로 한다. 일반 드래그는 사각형 영역을 확대하고, 폭/높이가 3px 미만인 축은 유지한다. Alt-drag는 오른쪽/위쪽으로 각각 시간/값을 확대하고 반대 방향은 축소한다. Auto Zoom Height가 켜진 상태에서는 모든 제스처의 세로 확대를 제한한다. 조작 기준은 [Adobe Graph Editor 문서](https://helpx.adobe.com/au/after-effects/desktop/animate-in-after-effects/animation-basics/animation-basics.html)를 참고했다.
- 확대는 키 위와 잠긴 레이어에서도 키·선택·현재 프레임·Undo 기록을 바꾸지 않는다. 일반 영역 확대는 릴리스 시 적용하고 Alt-drag는 시작 위치에서 계산해 누적 오차를 방지한다. Escape는 시작 화면으로 복원한다. 그래프 밖 릴리스 위치를 처리하며 영역은 그래프 경계에 제한한다. 시간 확대는 기존 1–64×와 정수 시작 프레임을 사용하므로 포인터 시간 보존에는 최대 0.5프레임 반올림 오차가 있다. 컴포지션 경계에서는 시작 범위가 추가로 제한된다.
- 자동 검증: 코어 154개 + 데스크톱 193개(총 347개) 통과, 외부 미디어/장치 30개 제외. 클릭/Alt-click의 포인터 기준점, 양방향 영역 드래그·단일 축·경계, 자동 높이 제한, Alt-drag 반복 이벤트·취소·최대 배율, 잠금·선택·Undo/Redo 보존, 저장 왕복과 Preview/출력 픽셀을 검사했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치), 릴리스 기존 경고 17개.
- 릴리스 CLI에서 수동 확대/자동 높이의 화면 메타데이터를 가진 두 QA 사본을 열어 30/40/50/55프레임 PNG 총 8개가 기존 출력과 동일함을 확인했다(`target/qa/verify_graph_zoom.py`). 이는 메타데이터 출력 회귀이며 실제 확대 제스처 검증을 대신하지 않는다. 사용자 보존 원본 SHA256 불변과 실제 편집기 프로세스 한 개를 확인했다.
- 후속 네이티브 검증: 이전 창 활성화 도구 오류가 해소되어 1442px 폭 실제 창에서 키 위 클릭 확대(30–66 → 40–58프레임), F 전체 맞춤, 영역 확대(34–57프레임), 키 선택/값 보존, V 복귀·F9 편집·Undo·저장을 확인했다. `target/qa/graph-zoom-native.lfe.json`에 배율 6.7708335와 높이 −1002.9661016949153…552.9661016949153을 저장했고 새 릴리스의 1920px 창에서 재열기·50프레임 Position X=1020을 확인했다. 해당 실제 파일의 30/40/50/55프레임 출력 PNG는 이전 QA와 동일하다(`target/qa/verify_graph_zoom_native.py`). Alt 조합 클릭/드래그, 자동 높이 상태 확대, 드래그 도중 Escape의 네이티브 검증은 남아 있다. 다중 채널·선택 변형 박스·시간 배율·공간 속도 등 D03 잔여 범위를 유지한다.

### 그래프 손 도구·중간 버튼 이동 — 2026-10-03

- D03 그래프에서 H 손 도구 드래그 또는 도구에 관계없는 중간 버튼 드래그로 화면을 이동한다. 고정 높이에서는 시간·높이 두 축, Auto Zoom Height에서는 시간만 이동한다. 키 위에서 시작하거나 레이어가 잠겨 있어도 선택·키 값·현재 프레임을 바꾸지 않는다. 기존 패널 배치와 아이콘을 유지한다.
- 시작 위치 기준의 절대 이동량을 사용해 반복 이벤트에서 누적 오차가 생기지 않으며 시간 범위를 컴포지션 경계로 제한한다. 시작한 버튼을 놓아야 종료하고, 그래프 바깥에서 놓은 최종 위치도 반영한다. Escape는 시작 전 화면 범위를 복원한다. 화면 이동은 문서·Undo 기록·렌더 결과를 바꾸지 않는다.
- 검증: 코어 154개 + 데스크톱 190개(총 344개) 통과, 외부 미디어/장치 30개 제외. 두 버튼·자동/수동 높이, 잠긴 레이어·선택·문서·Undo/Redo 보존, 반복 이동·취소·시간 경계, 화면 메타데이터 저장 및 Preview/출력 픽셀을 검사했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 키 위 손 도구 드래그로 27–63 → 30–66프레임 이동과 키 선택·값 보존, 자동 높이에서 세로 드래그 무시를 확인했다. 그래프 바깥 릴리스의 마지막 이동 누락을 수정하고 27–63 → 8–44프레임으로 이동되는 것을 확인했다. V 복귀·F 전체 맞춤·F9 편집·Undo가 정상 동작한다. `target/qa/graph-pan-native.lfe.json`에 30–66프레임과 높이 −798.75…1037.25를 저장·재열기했고, 30/40/50/55프레임 CLI PNG는 기존 QA와 픽셀이 동일하다. 사용자 보존 원본 SHA256 불변 및 작업 창 1개를 확인했다.
- 네이티브 중간 버튼 드래그와 드래그 도중 Escape는 자동 입력 도구 제약으로 미검증이며 관련 상태 계산은 단위 테스트로 확인했다. Space 임시 손 도구·Zoom 도구·다중 채널·선택 변형 박스·시간 배율·공간 속도는 남아 있다. D03 전체 완료로 표시하지 않는다.

### 그래프 화면 맞춤·이동·확대 — 2026-10-03

- 사용자 재개 요청에 따라 D03의 Auto Zoom Height, Fit Selection, Fit All을 그래프 하단 Gravity 아이콘에 연결했다. 선택 맞춤은 선택 키와 그 사이 곡선·방향 핸들의 세로 범위 및 키 시간 범위를 맞추고, 전체 맞춤은 표시 채널의 모든 키를 포함한다. 그래프 포커스에서 F/Shift+F로 전체/선택 맞춤을 실행한다. 키가 없는 전체 맞춤은 컴포지션 범위, 단일 키는 유효한 시간 폭을 사용하며 기존 타임라인 64× 제한을 따른다.
- 맞춤 후 높이는 고정된다. 휠은 세로 이동, Ctrl+휠은 포인터 기준 세로 확대, Shift+휠/가로 휠은 시간 이동, Alt+휠은 포인터 기준 시간 확대를 제공한다. Auto Zoom Height는 세로 이동·확대를 막고 현재 보이는 곡선을 다시 맞춘다. 키/핸들 드래그 중 휠은 무시한다. 범위 계산은 렌더와 키 히트 테스트가 공유한다. 동작 기준은 [Adobe의 Graph Editor 화면 조작](https://helpx.adobe.com/au/after-effects/desktop/animate-in-after-effects/animation-basics/animation-basics.html)을 참고했다.
- 그래프 종류와 고정 높이는 컴포지션별 선택적 화면 메타데이터에 저장한다. 프로젝트 버전·렌더 문서·Undo 기록은 바꾸지 않는다. 이전 파일은 자동 높이를 기본값으로 사용하고, 잘못된 높이 범위는 자동 높이로 복구한다. 그래프 종류를 바꾸면 새로운 단위에 맞춰 자동 높이로 전환한다. 수동 높이는 채널 변경 시 유지되므로 새 값 범위는 Fit All/Auto Zoom Height로 맞출 수 있다.
- 검증: 코어 154개 + 데스크톱 188개(342개) 통과, 외부 미디어/장치 30개 제외. 선택/전체/단일/빈 채널 맞춤, 음수 속도 양방향 끝점, 확대 포인터 보존·컴포지션 경계, 범위 유효성, 컴포지션 전환·Undo/Redo·저장 복원 및 문서 불변을 검사했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 Fit All 27–63프레임, 중간 키 선택 후 Shift+F 48–52프레임, F 복원, 세로 휠 이동·자동 높이 복원 및 세로 이동 제한, 가로 휠과 타임라인 동기화를 확인했다. 맞춤 화면에서 F9 편집·Undo/Redo도 확인했다. 1920/1442 폭에서 하단 도구를 확인했으며 패널 배치는 유지했다. QA 사본 `target/qa/graph-view-native.lfe.json`의 Speed 모드·고정 높이 −1143…693 저장·재열기를 확인했고, 30/40/50/55프레임 CLI PNG는 이전 원본과 픽셀이 동일했다.
- 네이티브 Ctrl/Alt/Shift+휠 조합은 도구 입력 제약으로 미검증이며 계산은 자동 테스트로 확인했다. 다중 채널, Hand/중간 버튼 직접 패닝, 선택 변형 박스·시간 배율, 공간 속도 등 D03 잔여 작업은 계속 남아 있다. D03 전체 완료로 표시하지 않는다.

### 그래프 시간·값 스냅 — 2026-10-03

- D03 그래프의 Snap 버튼을 기존 작업 영역 Snapping 설정과 연결했다. 키 드래그 시작 전 재생 헤드, 컴포지션/작업 영역 경계, 레이어 In/Out, 마커 경계와 다른 키 시간에 8 논리 픽셀 이내로 맞춘다. 값은 현재 채널의 미선택 키 값에 맞추며 Speed Graph에서는 부호 있는 속도를 사용한다. 일치한 시간/값은 주황색 안내선으로 표시한다. 동작 기준은 [Adobe Graph Editor 설명](https://helpx.adobe.com/au/after-effects/desktop/animate-in-after-effects/animation-basics/animation-basics.html)을 참고했다.
- 선택 그룹 전체에 같은 시간/값 차이를 적용하고 시간 충돌 후보를 건너뛴다. 움직이지 않은 축과 Shift로 고정한 축은 스냅하지 않는다. Ctrl은 드래그 중 설정을 반전하고 Alt는 스냅을 해제한다. 방향 핸들 조작에는 적용하지 않는다. 그래프의 Undo/Redo 후 포커스와 Keyframe 팝업 Escape 닫기도 보정했다.
- 검증: 코어 154개 + 데스크톱 184개 통과, 외부 미디어/장치 30개 제외. 화면 거리/확대율, 경계·충돌, 음수 속도/FPS, 그룹 간격, Undo/Redo·JSON 복원·미리보기/출력 픽셀 일치를 검사했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치), 기존 경고 17개.
- 실제 Windows에서 10/30프레임의 값 600/900인 두 키를 드래그하여 30/50프레임, 값 720/1020으로 맞췄다. Snap 해제 시에는 근처 값으로 붙지 않고 31프레임·733.83으로 이동했다. Undo/Redo, 팝업 Escape, 저장·재열기를 확인했다. QA 사본 `target/qa/graph-snap-native.lfe.json`의 30/40/50/55프레임 CLI PNG 중심은 독립 선형 계산 720/870/1020/870px와 일치한다. 사용자 보존 원본 SHA256 불변과 실제 작업 창 1개를 확인했다.
- Ctrl/Alt/Shift를 누른 채 드래그하는 네이티브 동작과 드래그 도중 안내선 캡처는 도구 입력 제약으로 별도 검증하지 못했다. 관련 계산은 자동 테스트로 검사했다. 다중 채널·Fit Selection/All 등 D03 잔여 범위는 유지한다. 사용자 요청에 따라 이번 기능 마무리 후 개발을 일시 중지한다.

### 그래프 방향 핸들 직접 편집 — 2026-10-03

- D03의 선택 키에 incoming/outgoing 방향 핸들을 표시한다. Value Graph는 실제 시간 Bezier 제어점, Speed Graph는 영향도에 따른 수평 길이와 초당 부호 있는 속도를 나타낸다. 핸들은 속이 빈 마름모로 키의 사각형과 구분하고 기존 타임라인 그래프 영역을 사용한다. 드래그는 해당 키/방향을 편집하며 시간·속성값·다른 방향의 영향도를 유지한다.
- Shift 드래그는 속도를 유지하며 영향도를 바꾸고, Alt는 Auto/Continuous 연결을 해제한 뒤 한쪽을 편집한다. 기본 드래그는 기존 연결 규칙을 따른다. 영향도는 0.1–100%이며 없는 끝점 구간과 Hold/수직 구형 접선에는 유한 핸들을 만들지 않는다. 숫자 필드로 초기화할 수 있다. Alt로 분리한 핸들의 자동 재결합과 여러 핸들 동시 조절은 남아 있다. 동작 기준은 [Adobe의 방향 핸들·속도 설명](https://helpx.adobe.com/after-effects/desktop/animate-in-after-effects/speed-between-keyframes/speed.html)을 참고했으며 AE의 내부 수치 보간과 동일하다는 의미는 아니다.
- 드래그 중 실제 코어 보간으로 곡선을 미리 표시하고 릴리스 시 한 번의 Undo로 확정한다. Escape, 문서·레이어/채널·그래프 종류 변경은 미확정 드래그를 취소한다. 선택 핸들을 높이 범위에 포함하며 드래그 중 좌표계를 고정한다. 극단 영향도에서 핸들이 겹치는 경우 숫자 필드를 사용할 수 있다.
- 검증: 코어 154개 + 데스크톱 181개(총 335개) 통과, 외부 미디어/장치 30개 제외. 양방향/Value·Speed 좌표 변환, Shift 속도 유지·영향도 경계, Auto/Continuous·Alt 분리·반대 영향도 보존, 드래그 미리보기와 확정 명령 일치, 잠금/유효하지 않은 값의 원자적 거부, Undo/Redo·저장 왕복·Preview/출력 픽셀을 검사했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치).
- 실제 Windows에서 Value outgoing 드래그로 40프레임·값 960을 유지하며 outgoing 영향도 58.717957%, 양쪽 속도 10.381950/s로 변경하고 incoming 70% 유지·Undo/Redo를 확인했다. 이어 Speed incoming 드래그로 incoming 81.538462%·양쪽 속도 186.720729/s·outgoing 58.717957%를 확인했다. QA 사본은 `target/qa/tangent-value-native.lfe.json`, `target/qa/tangent-native.lfe.json`이다. 두 사본의 20/50프레임 CLI 출력 중심은 독립 계산값 887.683398/873.834300px 및 819.675999/903.898196px와 출력 해상도 1px 이내로 일치한다.
- 최종 릴리스에서 `tangent-native`를 재열고 20/50프레임 미리보기 Position X 819.68/903.90과 저장된 그래프 복원을 확인했다. 실제 AE 창의 기본 Project/Composition/Timeline/우측 패널 구조도 대조했고 기존 배치는 변경하지 않았다. 작업 창 1개와 사용자 보존 원본 SHA256 불변을 확인했다.
- 네이티브 Shift/Alt 조합 드래그와 드래그 도중 Escape 입력은 도구에서 분리 입력이 지원되지 않아 미검증이다. 조합의 계산·명령은 단위 테스트로 확인했다. Undo/Redo 후 그래프 포커스가 없어 팝업 Escape가 작동하지 않는 경우도 후속 보정 대상으로 남긴다(Close 버튼은 작동). D03 전체 완료가 아니며 다중 채널·그래프 스냅·선택 변형/시간 배율·공간 속도는 남아 있다.

### 그래프의 다중 키프레임 편집 — 2026-10-02

- D03에서 현재 채널의 여러 키를 선택하고 함께 편집할 수 있다. Shift/Ctrl-click 선택 토글, 빈 그래프 영역의 드래그 선택과 Shift/Ctrl 추가 선택, Ctrl+A 전체 채널 선택을 연결했다. 선택 수와 파란 키 표시를 기존 그래프 영역에 표시하며 AE형 패널 배치를 유지한다.
- 선택한 키 하나를 드래그하면 시간 간격과 값 차이를 유지하며 전체가 움직인다. Shift는 우세한 축으로 제한한다. 컴포지션 양 끝에서는 그룹 전체를 제한하고, 선택한 원래 프레임끼리 겹치는 이동은 허용한다. 선택하지 않은 키와 충돌하거나 값이 유효하지 않으면 전체 명령을 취소한다. Escape와 문서 변경은 진행 중 드래그를 취소한다.
- Speed Graph는 선택한 방향의 속도에 동일한 차이를 적용하고 영향도·키 값을 유지한다. 해당 방향 구간이 없는 끝점 키는 시간만 이동한다. 보간·Auto/Continuous·Ease 버튼과 Delete는 표시 채널의 선택 키 전체에 한 번의 Undo로 적용한다. 숫자 필드는 활성 키 하나를 편집하고 다른 선택을 유지한다. 다른 채널/레이어에 숨은 선택 키를 그래프 명령으로 변경하지 않는다.
- 검증: 코어 154개 + 데스크톱 176개(총 330개) 통과, 외부 미디어/장치 30개 제외. 선택 토글·현재 채널 필터·역방향 영역 경계, 그룹 범위 제한·충돌·잠금/잘못된 값의 원자적 거부, 속도 차이·자동 보간 메타데이터·Undo/Redo·JSON 왕복, 그룹 편집 후 Preview/출력 픽셀을 검사했다. Speed Graph의 가로 이동만으로 Auto가 수동 보간으로 바뀌지 않도록 보정했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치).
- 실제 Windows에서 드래그 영역 선택 → 두 키를 +10프레임/+76.358443 이동 → Undo/Redo, Ctrl+A → F9/삭제 → Undo, 활성 키 숫자 편집 시 나머지 선택 유지를 확인했다. Undo로 선택이 해제된 뒤 이전 키 숫자 필드가 남던 문제도 수정했다. Shift/Ctrl 마우스 조합은 단위 테스트로 확인했으며 네이티브 조합 입력 검증은 남아 있다.
- 최종 릴리스에서 QA 사본 `target/qa/multikey-native.lfe.json`을 다시 열어 25/60프레임 Position X 1109.46/876.36을 확인했다. CLI 출력 도형 중심도 독립 계산값 1109.463643/876.358443px와 출력 해상도 1px 이내로 일치한다. Speed Graph의 그룹 outgoing 드래그는 시간·값·영향도를 유지하고 속도를 같은 차이만큼 변경하며 한 Undo로 복원한다. 속도 변경 사본은 `target/qa/multikey-speed-native.lfe.json`이다. 작업 창 1개와 사용자 보존 원본 SHA256 불변을 확인했다.
- 남음: 여러 채널/레이어 그래프 동시 표시·편집, 그래프 전용 스냅·선택 변형 박스/시간 배율·영향도 포인터 핸들, 공간 속도와 AE 수치 보간 동등성. D03 전체 완료로 표시하지 않는다.

### 자동·연속 시간 보간 — 2026-10-02

- D02의 스칼라 Auto Bezier / Continuous / Independent 모드를 Graph → Keyframe...에 연결했다. Auto는 주변 키의 값·시간 변경 때 접선을 다시 계산한다. 수동 속도/영향도 편집은 Continuous로 전환하며, 양쪽 속도는 연결하고 각 영향도는 독립적으로 유지한다. Independent로 바꾸면 현재 자동 접선을 고정한다.
- Auto 계산은 단조 구간의 가중 조화평균, 극값·평탄 접합점의 0 기울기, 끝점의 인접 할선, 영향도 1/3을 사용한다. AE의 내부 수치 알고리즘과 동일하다는 의미가 아니다. 동작 구분은 [Adobe의 키프레임 보간 설명](https://helpx.adobe.com/uk/after-effects/desktop/animate-in-after-effects/animation-keyframes/keyframe-interpolation.html)을 참조했다.
- Continuous 선택 시 기존 양쪽 유한 기울기의 평균으로 연결한다. 키 이동/값 편집은 수동 기울기를 유지한다. Linear/Hold는 관련 두 끝점의 자동 접선을 고정한 뒤 선택 구간을 초기화하며, Ease/Ease In/Ease Out은 연결을 해제한 뒤 지정 방향만 적용한다. Speed Graph의 속도 드래그도 같은 명령을 사용한다.
- 모드는 Undo/Redo·키/레이어 복사·저장·효과 프리셋에 보존한다. FPS 변환은 초당 수동 속도를 유지한다. 연결 모드는 프로젝트 v36 / 효과 프리셋 v3으로 보호하며, 기존 v35 독립 핸들과 구형 곡선은 그대로 읽는다.
- 검증: 코어 154개 + 데스크톱 169개 통과, 외부 미디어/장치 30개 제외. 자동 접선의 이웃 재계산·단조/평탄/감소·끝점, Continuous 연동·독립 영향도, 저장/구버전 거부·잠금 원자성, FPS 환산, Speed Graph 드래그와 한 번의 Undo, Preview/출력/재열기 픽셀을 검사했다. Cargo check/fmt/test/release 사용(Moon/proto 미설치).
- 실제 Windows에서 Auto 적용 → outgoing −300/s 입력 → Continuous 전환과 양쪽 −300/s → Undo/Redo → incoming 영향도 70% / outgoing 33.333333% 유지 → 별도 QA 저장을 확인했다. 긴 Auto 안내문이 닫기 버튼을 밀어내던 부분은 한 줄로 줄였다.
- 최종 릴리스 재실행·재열기에서 모드/속도/영향도 복원을 확인했다. Ease In은 Independent로 바꾸고 incoming만 0/s로 만들며 outgoing −300/s를 유지한다. Auto 안내문과 닫기 버튼도 같은 팝업에 표시된다. QA 사본 `target/qa/modes-auto-native.lfe.json`, `target/qa/modes-native.lfe.json`의 20/50프레임 출력 중심은 독립 계산값(Auto 825/825px, Continuous 1016.422454/800px)과 출력 해상도 1px 이내로 일치했다. 실제 50프레임 미리보기도 Position X 800px다. 작업 창 하나와 사용자 보존 원본 SHA256 불변을 확인했다.
- 남음: 다중 키/채널 그래프 편집, 영향도 포인터 핸들, 공간 경로와 다차원 속도, AE의 수치 보간 동등성. D02/D03 전체 완료로 표시하지 않는다.

### Speed Graph와 속도 핸들 드래그 — 2026-10-02

- D03의 단일 스칼라 Speed Graph를 구현했다. 기존 타임라인 그래프 영역에서 Value Graph / Speed Graph로 전환하며, 선택 채널의 실제 보간을 미분한 초당 부호 있는 속도를 표시한다. 별도 Position X/Y는 스칼라 채널이며 결합 공간 경로의 속력과 구분한다.
- 키 경계의 앞·뒤 속도는 분리된 선과 왼쪽/오른쪽 마커로 표현한다. Hold 점프·수직 탄젠트는 유한한 스파이크를 그리지 않으며, 0 기준선과 자동 높이 맞춤을 유지한다. Linear/Smooth/기존 Bezier/독립 시간 핸들이 같은 속도 계산을 사용한다.
- 속도 마커를 가로로 드래그하면 키 시간, 세로로 드래그하면 해당 방향 속도를 편집한다. 속성값과 영향도를 보존하며 충돌/잘못된 값은 전체 작업을 거부하고, 한 번의 Undo와 Escape 취소를 지원한다. 숫자 편집과 F9도 같은 그래프에서 사용할 수 있다. 그래프 종류 전환은 프로젝트를 수정하지 않는다.
- 검증: 코어 148개 + 데스크톱 168개 통과, 외부 미디어/장치 30개 제외. 해석적 미분과 수치 차분/적분 대조, 좌우 극한·Hold 점프·수직/제거 가능한 특이점, 곡선 분리, 시간/속도 편집의 값 보존·충돌 원자성·Undo를 검사했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치).
- 실제 Windows에서 Value/Speed 전환 후 중간 키를 30→40프레임으로 옮기며 outgoing을 −900→−507.919327 units/s로 변경했다. 원래 값 960과 incoming 영향도 70% 유지, Undo/Redo, 저장·재열기를 확인했다. incoming의 수직 드래그에서는 키 시간이 40으로 유지되고 해당 방향 속도만 변했다. QA 사본은 `target/qa/speed-native.lfe.json`이다. 저장된 20/50프레임을 CLI PNG로 렌더한 도형 중심은 독립 계산과 출력 1px 이내로 일치하며, 재열기 후 50프레임 미리보기 X=782.67도 계산값 782.673389와 일치했다.
- 남음: 다중 채널·다중 키 편집, 결합 공간 속력, Auto/Continuous 탄젠트, 영향도 핸들 드래그, 그래프 종류 자동 선택·설정 영속화. 최종 합성 픽셀이나 효과 출력 제한의 미분이 아닌 원본 속성 트랙의 속도다. D02/D03 전체 완료로 표시하지 않는다.

### 양방향 키프레임 속도와 영향도 — 2026-10-02

- D02의 독립 incoming/outgoing 시간 핸들을 구현했다. Graph → Keyframe...에서 부호 있는 초당 속도와 영향도(0.1–100%)를 각 방향별로 입력한다. 마지막 키의 incoming과 양 끝값이 같은 구간의 오버슈트도 지원한다. Transform·효과·Time Remap·오디오·마스크 숫자 트랙이 같은 모델을 사용하며 경로 포즈 트랙은 제외한다.
- F9/Ease는 선택한 키의 양쪽, Ease In은 incoming, Ease Out은 outgoing에 속도 0·영향도 1/3을 한 번의 Undo로 적용한다. Linear/Hold는 선택한 outgoing 구간의 두 끝 핸들만 초기화한다. 기존 파일의 Linear/Hold/Smooth/Bezier 샘플은 편집 전까지 그대로 유지하며, 독립 핸들을 사용한 구간은 다른 곡선을 보여줄 수 있는 기존 정규화 핸들 편집기를 숨긴다.
- 키의 값 변경·이동·복사·Undo/Redo·저장 복원에 핸들을 포함한다. 레이어 붙여넣기와 효과 프리셋의 FPS 변환은 초당 속도를 보존한다. 프로젝트 v35/핸들이 있는 효과 프리셋 v2로 보호하며, 구버전 표기·잘못된 값·잠금/없는 구간 편집은 원자적으로 거부한다.
- 실제 Windows에서 중간 키 F9 → Undo/Redo, incoming 영향도 70%, outgoing 속도 −900 입력·독립 그래프 변화·저장을 확인했다. QA 사본 `target/qa/temporal-native.lfe.json`을 CLI로 렌더하여 15/45프레임 도형 중심이 독립 계산값(891.989583/712.5px)과 출력 해상도 1px 이내로 일치함을 확인했다.
- 검증: 코어 146개 + 데스크톱 165개 통과, 외부 미디어/장치 30개 제외. 기존 보간의 정확한 샘플, 평탄 구간 오버슈트·끝점 미분, 저장 버전·잠금 원자성, 편집·복사·FPS 환산, Preview/출력/저장 복원 픽셀을 검사했다. Cargo check/fmt/test/release build 통과(Moon/proto 미설치). 사용자 보존 원본 SHA256 불변을 확인했다.
- 최종 릴리스 재실행·재열기 뒤 그래프 곡선과 incoming 70%/outgoing −900 값을 실제 창에서 확인했다. 미리보기의 15/45프레임 Position X도 891.99/712.50으로 출력 계산과 일치했다. 후속 실행 시 이전 작업 창이 닫히고 작업 창은 하나만 남는다.
- 남음: Auto/Continuous 연결, Speed Graph·다중 키/채널 편집, 독립 핸들 포인터 드래그, 경로 시간 핸들, AE의 다차원 속도·전체 Easy Ease 호환성. D02/D03 전체 완료로 표시하지 않는다. 속도·영향도 기준은 [Adobe 문서](https://helpx.adobe.com/after-effects/desktop/animate-in-after-effects/speed-between-keyframes/speed.html)를 참조한다.

### 명령 검색과 실행 — 2026-10-02

- Ctrl+Shift+P 또는 Help → Find command로 명령 검색을 연다. 메뉴·명령 이름·표시 단축키의 대소문자 구분 없는 단어 검색, 편집 도구/Shape/기본 Preview 명령, 비활성 결과 표시, 상하 선택·Ctrl+Home/End·Enter 실행·Escape/바깥 클릭 취소를 제공한다. 기존 패널 배치를 바꾸지 않는다.
- 결과 행에 저장한 예전 Action을 실행하지 않고 메뉴/명령 식별자로 현재 문서와 선택에서 다시 찾는다. 레이어 선택 변경과 잠금으로 오래된 효과 대상이 실행되지 않도록 검사했다. 검색 입력은 문서 이동/재생/저장 단축키와 분리하며 IME marked 범위가 있는 동안 Enter/방향키를 검색 실행으로 처리하지 않는다.
- 검증: 코어 140개 + 데스크톱 164개 통과, 외부 미디어/장치 30개 제외. 단어·분류·단축키 검색, 결과 식별자 중복 없음, 비활성 결과 건너뛰기, 선택/잠금 변경 시 재해석, 검색한 효과 명령의 Undo/Redo·JSON 왕복·Preview/출력 픽셀 일치를 검사했다. Cargo check/fmt/test/release build 사용(Moon/proto 미설치).
- 실제 Windows에서 검색 열기·입력·사각형 생성·Gaussian Blur 추가·Undo/Redo와 별도 파일 저장을 확인했다. 검색 결과의 빈 버튼 자식 때문에 명령 이름이 가운데로 밀리던 문제와 정지한 포인터의 hover가 키보드 선택을 바꾸던 문제를 수정했다. QA 사본은 `target/qa/command-search-native.lfe.json`에 보관한다.
- 최종 릴리스에서 재열기 후 Gaussian Blur 10px를 확인하고, 검색 결과 왼쪽 정렬·Down 선택·Ctrl+End 자동 스크롤·Escape 취소 및 문서 불변을 확인했다. 이전 메뉴 검증의 누락 범위 중 F10 → Left(Help 순환) → Down → Enter(도움말 실행) → Escape도 실제 창에서 확인했다. CLI 960×540 PNG는 저장된 Blur를 반영하며, 효과를 제거한 QA 사본과 비교해 부드러운 알파 경계 13,288개가 생긴다. 작업 창 1개와 사용자 보존 원본 불변을 확인했다.
- 남음: 단축키 사용자 설정·다국어 명령 별칭·최근 명령·패널 내부의 모든 컨텍스트 명령 등록, 실제 IME 후보 창/화면 읽기/DPI 회귀. 검색 범위는 등록된 작업 명령이며 모든 AE 명령을 포함한다는 의미가 아니다. B05/B07 전체 완료로 표시하지 않는다.

### 상단 메뉴 키보드 조작 — 2026-10-02

- B05의 메뉴 탐색을 구현했다. F10 진입, 좌우 메뉴 전환, 상하 비활성 항목 건너뛰기/순환, Home/End, 첫 글자 반복 탐색, Enter/Space 실행, Escape/F10/Tab 닫기와 이전 포커스 복원을 연결했다. 긴 메뉴는 창 높이에 맞춰 스크롤하고 선택 항목을 보여준다.
- 마우스·키보드가 같은 메뉴 항목/실행 경로를 사용한다. Composition의 생성·복제·삭제·설정, Window 초기화, Help도 같은 모델에 포함했다. File은 New/Open/Save부터 시작하고 출력 항목은 뒤에 둔다. 기존 AE형 패널 배치·Wanted Sans·Gravity Icons를 유지한다.
- 메뉴가 열린 동안 방향키·Delete·문서 단축키가 편집기로 전달되지 않도록 캡처한다. 입력 필드와 직접 텍스트의 IME marked 범위가 있는 동안 F10 진입을 막고, 모달에서도 진입하지 않는다. 일반 입력 초안은 메뉴를 열 때 기존 편집 경로로 확정한다.
- 검증: 코어 140개 + 데스크톱 162개 통과, 외부 미디어/장치 30개 제외. 메뉴 순환·비활성 건너뛰기·첫 글자 검색·잠금/선택에 따른 효과 메뉴·특수 명령 연결·메뉴 편집 명령의 저장 왕복과 Undo/Redo를 검사했다. Cargo check/fmt/test/release build를 사용한다.
- 네이티브 검증은 부분 완료: 실제 Windows에서 F10으로 메뉴가 열리고 첫 항목이 강조되는 것을 확인했다. 다음 입력에서 Computer Use가 `failed to activate captured window`를 반환했고 새 창 재선택/재활성화도 실패했다. 따라서 방향키/Enter 실행·입력 포커스 복원·실제 IME·DPI/스크롤 회귀는 검증된 것으로 표시하지 않는다. B05/B07 전체 완료가 아니며 명령 검색·단축키 설정·화면 읽기 의미 정보도 남아 있다.
- 최종 릴리스의 두 컴포지션 CLI 출력은 기존 폰트 교체 QA와 픽셀이 동일했다. 실제 작업 창 프로세스 1개와 사용자 보존 원본 SHA-256 불변을 확인했다.

### 프로젝트 글꼴 진단과 일괄 교체 — 2026-10-02

- File → Manage project fonts에서 모든 컴포지션의 글꼴/실제 스타일/굵기/기울임 조합을 모아 사용 레이어·잠금 상태·누락 여부·주 대체 face를 표시한다. 누락된 글꼴/스타일이 있는 문서를 열면 상태 표시줄에서 안내한다.
- 설치된 글꼴 검색과 실제 스타일 선택 후 일치하는 잠금 해제 레이어를 한 번에 교체한다. 잠긴 레이어와 원래 누락 글꼴 이름을 보존하며, 다른 컴포지션 변경까지 한 Undo/Redo로 복원한다. 텍스트·크기·간격·채우기/윤곽선·문단 박스·애니메이션 속성은 보존한다. 글리프 폭과 줄바꿈은 선택한 글꼴에 따라 달라질 수 있다.
- 이전 문서/변경된 문서에 대한 교체 계획을 거부한다. 문서 전환 번호만으로 목록을 갱신해 일반 편집 직후 목록이 오래된 상태로 남던 문제를 네이티브 검사에서 찾아 프로젝트 내용 비교로 수정했다. 저장 스키마는 v34를 유지한다.
- 검증: 코어 140개 + 데스크톱 159개 통과, 외부 미디어/장치 30개 제외. Cargo check/fmt/test/release build 통과(Moon/proto 미설치). 다중 컴포지션·잠금·실패 원자성·Undo/Redo·저장 왕복·대체 출력 픽셀 보존을 검사했다.
- 실제 Windows에서 누락 그룹 검색/교체, 두 컴포지션의 잠금 해제 레이어 2개 변경과 잠긴 레이어 1개 보존, Undo/Redo·저장·재열기를 확인했다. 최종 릴리스에서 Bold → Regular 교체 직후 목록/Character 갱신과 한 Undo 복원을 확인했다. 릴리스 CLI로 두 컴포지션의 교체 전후 960×540 PNG가 모든 픽셀에서 동일함을 확인했다(누락 Bold → 실제 Wanted Sans Bold).
- 최종 편집기 작업 창 1개와 사용자 보존 원본 SHA-256 불변을 확인했다. QA 문서는 `target/qa/project-fonts-native.lfe.json`에 별도로 저장한다.
- 남음: 글리프별 대체/문자 누락 진단, 가변 글꼴 축, 실행 중 글꼴 카탈로그 갱신, 문자별 스타일, 다른 PC/OS 간 실제 이식성 검증. 글꼴 파일을 내장하거나 수집하지 않는다. F02 전체 완료로 표시하지 않는다.

### 텍스트 채우기와 윤곽선 — 2026-10-02

- Character에 Fill/Stroke 스위치, Stroke 색상(HEX/공통 색 선택기), 0–1000px 두께, Miter/Round/Bevel 모서리, 전체 채우기/전체 윤곽선 겹침 순서를 연결했다. 순서·모서리 버튼은 클릭 또는 Enter/Space로 순환한다. Wanted Sans와 기존 패널 경계를 유지한다.
- 윤곽선은 글리프 경로 중앙 기준이며 커서 위치·글자 간격·문단 줄바꿈을 바꾸지 않는다. 문단 경계의 클리핑과 Point text 효과 영역의 윤곽선 확장을 반영한다. v34 저장 왕복, 잠금/잘못된 값 거부, Undo/Redo를 지원한다.
- 검증: 코어 139개 + 데스크톱 157개 통과, 외부 미디어/장치 30개 제외. Cargo check/fmt/test/release build 통과. 겹치는 글자/행의 두 합성 순서를 별도 이미지 합성과 대조했고, 채우기 없음·모두 끄기·0px·모서리별 픽셀 차이·문단 밖 알파 0·커서/줄바꿈 불변을 검사했다. 릴리스 CLI 세 변형 PNG와 기존 Point text 픽셀 동일성도 확인했다.
- 실제 Windows에서 Stroke 파일 열기, Fill 끄기, 14→4px 입력, 모서리 클릭/Enter 변경, 공통 색 선택기 HEX 적용, 색상 Undo/Redo, 저장·재열기를 확인했다. 저장된 4px/주황색/Miter/채우기 없음과 화면이 일치한다. 작업 창 프로세스 1개와 보호 원본 불변을 확인했다.
- 남음: 문자별 서식/윤곽선, 속성 애니메이션, 문자별 합성 순서, 조절 가능한 Miter limit, 컬러/비트맵 글꼴의 윤곽선 동등성, 전체 키보드/접근성 검증. F03 전체 완료로 표시하지 않는다.

### Paragraph 영역 텍스트 — 2026-10-02

- Text 도구의 드래그로 Paragraph 박스, Alt-드래그로 중심 기준 박스를 만든다. 편집 진입 전 Shift-클릭은 기존 글자 위에도 새 레이어를 만든다. 편집 중 오른쪽 아래 핸들 또는 Paragraph의 폭·높이 필드로 박스를 바꾸면 글자 크기를 유지하며 재배치한다.
- Unicode 줄바꿈 기회와 grapheme 경계를 사용한다. 긴 단어도 조합 문자/이모지를 분리하지 않으며, Enter와 Shift+Enter는 각각 문단/강제 줄바꿈을 입력한다. 자동 줄바꿈과 크기 변경은 원문을 바꾸지 않는다. 실제 렌더러의 대체 글꼴과 자간으로 각 줄 폭을 확인한다.
- 박스에 완전히 들어가는 줄만 출력하며 넘친 내용은 보관한다. 넘침 표시, 높이 맞춤, 시각적 줄 단위 Home/End와 줄바꿈 경계의 커서 위치를 연결했다. 편집 확정은 생성/크기 변경을 포함해 한 Undo, Escape는 초안 취소다. Point/Paragraph와 크기는 v33에 저장하며 기존 문서는 Point를 기본값으로 읽는다.
- Paragraph → Point는 보이는 줄마다 실제 개행을 추가하고 박스 밖 내용은 제거한다. 한 Undo로 원문/박스를 복원한다. [Adobe 변환 규칙](https://helpx.adobe.com/after-effects/desktop/add-text/create-and-edit-text-layers/creating-editing-text-layers.html)을 따르는 동작이므로 숨은 내용까지 유지하려면 먼저 박스를 확대한다.
- 검증: 코어 138개 + 데스크톱 154개 통과, 외부 미디어/장치 30개 제외. Unicode 재배치·강제 줄바꿈·좁은 박스·넘침 변환·원문 복구, 역방향/중심 드래그, 크기 변경 Undo/Redo, 저장 왕복·하위 스키마 거부·잠금/잘못된 크기, Preview/출력 픽셀·박스 밖 알파 0·명시적 줄바꿈과 픽셀 일치를 검사했다. Cargo check/fmt/test/release build 사용.
- 릴리스 CLI로 폭/높이가 다른 세 파일을 960×540 PNG로 출력해 재배치와 박스 밖 알파 0을 확인했다. 기존 Point text 출력은 이전 픽셀과 동일하다. 실제 Windows 창에서 파일 열기, Text 도구 진입, 오른쪽 아래 핸들 축소와 재배치, 전체 선택·한글/영문 입력, Ctrl+Enter, 문서 Undo/Redo, 저장된 v33·크기·원문을 확인했다. 현재 AE 창의 좌측 Project/중앙 Composition/우측 접이식 패널/하단 Timeline 구조도 대조했다. 픽셀 단위 UI 일치를 의미하지 않는다.
- 교체 실행 후 작업 창 프로세스 1개와 보호 원본 SHA256 불변을 확인했다. 실제 IME 조합/후보 창, 새 박스 생성과 Alt/Shift 제스처의 네이티브 검사, 전체 포커스/키보드 회귀는 남아 있다. 고급 문단 들여쓰기/문단별 스타일/양쪽 맞춤, 세로쓰기, 글꼴별 첫 기준선 옵션, 8방향 박스 핸들과 AE 텍스트 엔진의 모든 줄 배치 규칙도 남아 있다. F01–F03 전체 완료로 표시하지 않는다.

### 텍스트 선택 영역과 렌더 배치 일치 — 2026-10-02

- 커서 전용 글꼴 폭 계산을 렌더러가 실제 배치한 글리프 위치와 대체 글꼴을 읽는 방식으로 바꿨다. 출력 뒤에 없는 마지막 자간을 커서 폭에 추가하던 문제, 가운데/오른쪽 정렬의 끝 위치 오차, 선택 글꼴에 없는 한글·다른 문자로 생기던 폭 차이를 보정한다.
- 동일한 렌더 옵션과 SVG 텍스트 생성기를 사용한다. usvg가 공개하지 않는 원문 byte 위치만 실제 사용된 face의 shaping 메타데이터로 복원한다. 음수 자간으로 사라진 반복 글자도 실제 남은 문자의 인덱스에 대응시킨다. 조합 문자·ZWJ 이모지는 하나의 선택 단위로 묶는다.
- 마지막 배치를 한 항목의 캐시로 보관해 포인터 이동/선택 다시 그리기에서 재분석하지 않는다. 편집 중 더블클릭 단어/세 번 클릭 줄 선택, 짧은 줄을 지나도 가로 위치를 유지하는 위아래 이동을 추가했다. 패널 배치·문서 스키마·출력 규칙은 바꾸지 않는다.
- 검증: 코어 137개 + 데스크톱 149개 통과, 외부 미디어/장치 30개 제외. Wanted Sans/Arial, 3가지 자간, 한글·CJK·아랍어·히브리어·Devanagari·조합 문자·이모지·탭·bidi 제어 문자의 54개 조합에서 줄 끝이 실제 렌더 폭과 일치한다. 음수 반복 글자, 자간 정렬, 캐시 무효화, 단어/줄 선택과 세로 이동도 검사했다. Cargo check/fmt/test/release build로 검증한다.
- Windows 창 제어는 세션 재초기화와 새 창 정보 재선택 후에도 활성화 오류가 반복됐다. 실제 포인터/IME/포커스 조작은 여전히 미검증이다. 글꼴의 GDEF ligature 내부 caret 위치, bidi 경계 affinity, 커서 깜빡임, Paragraph 박스와 문자별 스타일/Text Animator는 남아 있다. 전체 F01–F03 완료를 의미하지 않는다.
- 최종 릴리스 검증: Arial 요청에 한글/CJK/이모지/히브리어를 섞고 자간 150·가운데 정렬을 사용한 `target/qa/text-metrics-cli.lfe.json`을 960×540 RGBA PNG로 출력했다. 이전/이후 릴리스의 모든 픽셀이 일치했다(`text-metrics-before.png`, `text-metrics-after.png`). 릴리스 교체 후 편집기 프로세스 1개와 사용자 보존 원본의 SHA-256 불변을 확인했다.

### Composition 직접 텍스트 편집 — 2026-10-02

- Ctrl+T/Text 도구로 클릭해 포인트 텍스트를 만들거나 기존 글자를 편집한다. Selection 도구의 더블클릭과 Layer → New text도 연결했다. 여러 줄이 소스 사각형 밖으로 길어져도 글자 영역에서 편집을 시작할 수 있다.
- 드래그/Shift 선택, 방향키·Home/End·문서/단어 이동, Unicode grapheme 단위 삭제, 복사/잘라내기/붙여넣기, 편집 중 로컬 Undo/Redo를 구현했다. Enter는 줄바꿈, Ctrl+Enter는 확정, Escape는 원래 내용 복원이다. 빈 새 초안은 레이어를 만들지 않는다.
- 실제 문서와 분리된 초안을 같은 렌더러로 미리 보여준다. 확정은 생성까지 한 Undo로 기록하고 기존 글꼴·변형 키를 보존한다. 다른 패널 클릭/도구/저장/시간 이동은 먼저 확정한다. 자동 복구는 초안을 포함하며 편집기 교체/닫기도 확정 후 기존 보존 흐름을 따른다. 프로젝트 스키마는 v32를 유지한다.
- 네이티브 입력 핸들러의 UTF-16 선택·조합 영역·후보 창 위치를 연결했다. 한글 조합 갱신, surrogate pair, 결합 문자·ZWJ 이모지, 16 KiB 제한의 원자성, 오래된 문서 거부를 모델 테스트로 검사했다.
- 검증: 코어 137개 + 데스크톱 145개 통과, 외부 미디어/장치 30개 제외. 저장 왕복·한 번의 Undo/Redo·Preview/PNG 픽셀 일치·글꼴/애니메이션 보존·복구 초안 재열기·여러 줄/정렬/기본 RTL hit 영역을 검사했다. Cargo check/fmt/release build 사용(Moon/proto 미설치).
- **네이티브 조작 검증은 미완료:** Computer Use의 창 활성화가 재선택/재시도 후에도 `failed to activate captured window`로 실패했다. 따라서 실제 IME·마우스 선택·포커스·키 입력·UI 저장/재열기는 검증됐다고 표시하지 않는다. 새 릴리스의 프로세스 교체와 편집기 프로세스 1개 유지는 별도 확인했다.
- 최종 릴리스 CLI로 별도 3줄 한글/영문 QA 문서를 960×540 RGBA PNG로 출력하고 글자·줄 간격을 확인했다(`target/qa/text-edit-cli.lfe.json`, `target/qa/text-edit-cli.png`). CLI는 정상 종료했으며 사용자 보존 원본의 SHA-256은 바뀌지 않았다. 이 검사는 UI 입력 검증을 대신하지 않는다.
- 남음: 대체 글리프/복잡한 양방향 문자/ligature의 커서 정밀도, 커서 깜빡임, 문자별 스타일, Paragraph 영역 텍스트, Text Animator. 현재 커서는 선택된 실제 face로 모양을 계산하여 렌더러의 글리프별 fallback과 차이가 날 수 있다. F01–F03 전체 완료가 아니다.

### 텍스트 글꼴과 스타일 선택 — 2026-10-02

- Character에 설치된 글꼴 검색·선택과 실제 스타일 메뉴를 추가했다. 목록은 떠 있는 메뉴로 열어 기존 오른쪽 패널 높이를 바꾸지 않는다. 앱 UI와 새 텍스트의 기본값은 Wanted Sans다.
- Wanted Sans Regular/Medium/SemiBold/Bold/ExtraBold/Black/ExtraBlack 7개 정적 파일을 포함한다. Black과 ExtraBlack은 같은 숫자 굵기 900을 선언하므로 PostScript 스타일 식별자도 저장하고 실제 글꼴 파일을 구분해 렌더링한다.
- 레이어 전체의 글꼴 이름·스타일 식별자·굵기·기울임을 v32에 저장하며 Undo/Redo와 이전 문서 기본값 변환을 지원한다. Preview·효과 경계 측정·PNG/영상 출력은 하나의 글꼴 카탈로그와 동일한 선택 로직을 사용한다.
- 없는 글꼴/스타일은 Character에서 경고하고 원래 저장된 이름을 보존한다. 없는 글꼴은 Wanted Sans, 없는 스타일은 가장 가까운 실제 스타일로 표시한다. 설치된 글꼴은 실행마다 한 번 검색하며 외부 글꼴을 프로젝트에 내장·수집하지 않는다.
- 검증: 코어 137개 + 데스크톱 139개 통과, 외부 미디어/장치 30개 제외. 7개 굵기의 서로 다른 한글/영문 픽셀, 없는 글꼴의 대체 결과, 저장/재열기·Undo/Redo·PNG와 Preview 일치, 잘못된 이름/굵기·잠금·하위 스키마 거부를 검사했다. Cargo check/fmt/release build로 검증하며 Moon/proto는 PATH에 없다.
- 남음: 캔버스 직접 텍스트 입력·선택·IME, 문자별 스타일, 가변 글꼴 축·문자 누락 진단·프로젝트 전체 글꼴 대체, Paragraph 영역 텍스트와 Text Animator. F01–F03 전체 완료로 표시하지 않는다.
- 네이티브 검증: ExtraBlack 선택·Undo/Redo 후 v32의 `WantedSans-ExtraBlack` 저장을 확인했다. 설치된 Arial을 검색해 Arial-ItalicMT로 변경·저장하고 릴리스 재시작/재열기 후 같은 스타일과 화면을 확인했다. 메뉴가 열려도 Project/Composition/Timeline/오른쪽 도크 경계는 유지된다.
- CLI 960×540 PNG에서 Regular/Black/ExtraBlack/Arial Italic 4개의 서로 다른 글리프 픽셀을 확인했다. 없는 글꼴의 출력은 Wanted Sans Regular 대체 출력과 모든 픽셀이 일치했다. `target/qa/typography-*-native.lfe.json`과 대응 PNG를 QA 사본으로 보관한다.
- 최종 릴리스에서 스타일 메뉴 및 검색창의 Escape 취소가 문서를 변경하지 않는 것을 확인했다. 실제 작업 창 1개를 유지한다.

### 효과 프리셋 저장·적용 — 2026-10-02

- D06 중 효과 애니메이션 재사용 범위를 구현했다. Effect Controls에서 전체 스택 또는 개별 인스턴스를 `.lfe-preset.json`으로 저장하고 Effects & Presets의 User Presets에서 검색·가져오기·새로고침·적용한다. 저장 시 사용자 라이브러리에도 등록하며 동일 파일을 다시 가져와도 중복 등록하지 않는다.
- 효과 순서·이름·우회 상태·색 공간·기본값·키프레임·보간을 보존한다. 첫 키를 적용 시점에 맞추고 나머지 키는 원본 FPS의 경과 시간을 대상 FPS로 변환한다. 좌표·반경은 원래 픽셀 단위를 유지한다. 정적 효과는 레이어 전체 구간에 적용된다.
- 기존 효과 뒤에 새 ID로 추가하며 선택한 여러 레이어의 적용을 한 번의 Undo/Redo로 처리한다. 잠금·Null·키 충돌·컴포지션 범위 초과·64개 효과 한도에 걸리면 전체 적용을 거부한다. 레거시 효과가 있는 전체 스택은 먼저 순서 있는 효과로 변환해야 저장된다.
- 프리셋 파일 버전은 1이며 프로젝트 스키마는 바뀌지 않는다. 파일당 8 MiB·40,000키, 라이브러리 200개·32 MiB를 제한하고 잘못된 파일은 경고와 함께 건너뛴다. 프로필 디스크 처리는 백그라운드로 실행한다.
- 아직 남음: Transform/Mask/Text 등 임의 속성 선택 프리셋과 선택 붙여넣기, 프리셋 폴더·이름 변경·삭제 UI, Adobe FFX 호환. D06 전체 완료로 표시하지 않는다.
- 검증: 코어 136개 + 데스크톱 137개 통과, 외부 미디어/장치 30개 제외. 시간/FPS·키 보간·ID·색 공간 보존, 다중 레이어 적용 실패의 원자성, Undo/Redo·프로젝트/프리셋 JSON 왕복, 애니메이션 중간 프레임 PNG, 잘못된 파일·중복 가져오기를 검사했다. Cargo check/fmt/release build도 통과했다(Moon/proto 미설치).
- 네이티브 검증: Gradient+Curves 2개 효과와 키 6개를 `target/qa/Warm curves.lfe-preset.json`으로 저장했다. 새 Solid의 30프레임에 적용해 0/45/89 키가 30/75/119로 이동했으며 U 표시와 한 번의 Undo/Redo를 확인했다. 네이티브 재가져오기는 한 항목을 유지했다. 적용 전 `target/qa/preset-before-native.lfe.json`의 0/15/45/89 PNG와 저장 후 `target/qa/preset-source-native.lfe.json`의 30/45/75/119 PNG는 각각 모든 픽셀이 일치했다.
- 최종 릴리스 재시작·문서 재열기 후 30프레임의 화면과 효과 키를 확인했다. User Presets의 Warm curves가 복원되고 이름 검색에도 표시된다. 실제 작업 창은 1개이며 원본 보존 문서의 SHA-256은 바뀌지 않았다.

### 공통 색 선택기 — 2026-10-02

- B06 기본 편집 범위: Properties/Character Fill, 도형 Stroke, Composition Settings 배경색에서 같은 RGB/HEX 선택기를 연다. 채도·명도 영역과 색상 띠, 원래/새 색 견본, 방향키·Page Up/Down·Shift 조작, 최대 12개 최근 색상의 로컬 프로필 저장을 제공한다.
- 레이어색의 RRGGBBAA/Opacity는 현재 프레임의 **레이어 전체 불투명도**를 바꾼다. 채우기만의 별도 알파 모델은 아니다. RGB와 불투명도는 한 Batch로 적용되어 한 번의 Undo/Redo로 복원된다. 도형 선과 배경은 RGB만 편집한다.
- OK 전에는 문서에 쓰지 않는다. 잘못된 숫자/HEX, 다른 문서·프레임, 잠긴 대상은 거부한다. 배경색 선택은 바깥 Composition Settings의 임시 값으로 반환하므로 바깥 Cancel도 안전하다.
- 스포이트는 현재 컴포지션 미리보기 해상도의 raw 8-bit straight RGBA를 읽으며, 격자·배경·채널 표시·선택선은 포함하지 않는다. 아직 OS 전체 화면 샘플, ICC/HDR 색 관리, 효과 RGB 매개변수 그룹의 공통 색 선택기 연결, 도형 도구 Fill/Stroke 기본값은 구현하지 않았다. 고급 색 관리는 사용자 요청대로 후속 단계다.
- 테스트: 코어 133개 + 데스크톱 135개 통과, 외부 미디어/장치 30개 제외. RGB/HEX/알파 오류의 원자성, HSV 왕복, 애니메이션 불투명도 키 보존, 한 번의 Undo/Redo, 잠금/오래된 세션 거부, 도형 선 보존, 배경 임시 값, 최근 색상 경계·저장, JSON/PNG 픽셀 왕복을 검사했다. Moon/proto가 PATH에 없어 Cargo check/fmt/test/release build로 검증한다.
- 네이티브: Solid에 #33669980을 입력해 50.20% 불투명도를 적용하고 Undo/Redo를 확인했다. 색상 영역 드래그 후 스포이트는 [52,102,153,128]을 읽었다(8-bit premultiply 반올림); Cancel로 원래 #336699를 유지했다. 최근 색상을 배경으로 선택한 뒤 바깥 설정 Cancel이 배경 0을 유지하는 것도 저장 파일로 확인했다. 테스트 문서는 target/qa/color-picker-native.lfe.json에 별도 보관한다. AE 실제 창과 첫 화면 패널 경계를 다시 비교했으며 기존 배치는 유지했다.

최종 릴리스 재시작 후 같은 문서를 재열어 Fill #336699·Opacity 50.20%와 최근 색상 복원을 확인했다. 방향키 색상 변경과 Escape 취소 후 문서는 저장 상태를 유지한다. CLI 320×180 PNG 중앙 픽셀은 알파 출력 [52,102,153,128], 검정 배경 합성 출력 [26,51,77]이다. 실제 작업 창 1개와 원본 보존 파일의 SHA-256 불변도 확인했다.

### 마스크 속성과 경로 애니메이션 — 2026-10-02

- 마스크마다 Opacity(0–100%), Feather(0–256px), Expansion(−256–256px), Path 스톱워치·키 추가/제거를 제공한다. Properties의 각 마스크 아래에 연산/반전/순서와 속성을 모았다. 수치 속성은 타임라인과 값 그래프에서도 편집한다.
- 마스크 고유 ID를 저장해 순서를 바꿔도 키프레임 대상이 유지된다. v29 정적 마스크는 읽을 때 한 번 변환한다. 속성은 v30, 경로 애니메이션은 v31에 저장한다.
- 도형/마스크 경로의 꼭짓점과 입출력 핸들을 보간한다. Path 애니메이션을 켜고 다른 시간에서 Pen으로 이동하면 해당 프레임에 키가 생긴다. Linear/Hold/Smooth/Bezier 시간 보간, 타임라인 키 이동·복사/붙여넣기·삭제·레이어 이동·Undo/Redo, 중간 모양을 유지하며 애니메이션 끄기를 지원한다.
- 경로 키 복사는 좌표와 핸들을 함께 복사한다. 다른 레이어의 같은 종류 경로로 붙여넣을 때 꼭짓점 수와 닫힘 상태가 일치해야 한다. 애니메이션 중 점 삽입/삭제와 닫힘 변경은 거부하고 안내한다. 경로는 숫자 값 그래프로 표시하지 않고 Composition의 Pen 편집으로 연결한다.
- Preview와 PNG/영상 합성은 같은 평가 경로를 사용한다. Feather는 균일 Gaussian, Expansion은 SVG morphology 방식이다. AE의 가변 Feather·정교한 경계 오프셋과 수치적으로 동일한 구현은 아니다.
- 이미지/시퀀스 자산 디코더의 버전 상한이 26에 고정되어 최신 편집 기능과 함께 저장한 프로젝트를 열지 못하던 문제를 수정했다. 지원되는 프로젝트 버전을 한 상수로 관리한다.
- 아직 남음: 경로 토폴로지 변경의 전체 키 전파, 다중 점 선택·수치 편집, 로토베지어/가변 Feather, Contents 그룹/연산, 도형 파라미터 키프레임, 직접 텍스트 편집·글꼴/굵기·문자별 스타일, 실제 색 선택기, 효과 프리셋과 추가 효과. 전체 AE 동등 기능 완료로 취급하지 않는다.

검증: 코어 133개 + 데스크톱 130개 테스트 통과, 외부 미디어/장치 테스트 30개 제외. 경로 중간 좌표/핸들·시간 보간·키 클립보드·저장·이력, Pen 한 제스처의 키 생성과 Undo, Shape/Mask 중간 프레임 픽셀, Feather/Expansion/Opacity와 순서 있는 마스크 합성, 최신 이미지·시퀀스 혼합 문서 재열기를 검사했다. 서로 다른 FPS로 레이어를 복사할 때 경로·마스크 수치 키가 함께 시간 변환되는 것도 검사했다. Cargo check/fmt/release build로 검증하며 Moon/proto가 PATH에 없어 Cargo를 직접 실행했다.

네이티브 검증: 이전 v29 테스트 문서를 열고 Path/Feather 스톱워치를 켰다. 30프레임에서 Feather 60과 꼭짓점 이동으로 두 키를 만들고 15프레임에서 Feather 30과 중간 위치를 확인했다. Pen 드래그 Undo/Redo, 경로 키를 60프레임에 복사, Feather 그래프의 시간·값 이동과 Undo, U 필터, v31 별도 저장을 확인했다. `target/qa/mask-animation-native.lfe.json`에는 Path 0/30/60, Feather 0/30 키가 있다. CLI로 0/15/30프레임을 PNG 출력해 부드러운 알파 경계 변화와 Subtract 구멍의 알파 0을 확인했다. 실제 After Effects 2026 첫 화면과 대조한 Project/Composition/Timeline/오른쪽 도크 경계는 유지했다. 최종 릴리스 재시작/재열기 후에도 같은 키와 모양이 복원됐고, 타임라인은 마스크별 Path/수치 속성을 연속 배치한다. 실제 작업 창은 1개다.

### Pen 경로와 다중 마스크 — 2026-10-02

- G 펜 도구: 클릭으로 꼭짓점, 드래그로 베지어 핸들, 시작점 클릭으로 닫기, Enter로 완료, Escape 취소, 작성 중 Backspace로 마지막 점 취소.
- 선택한 경로의 점/핸들 이동, 곡선 클릭으로 형태를 유지하며 점 삽입, 선택한 점 Delete, Alt로 모서리 전환/독립 핸들, Shift로 핸들 축 제한. 작성과 각 드래그는 한 Undo로 처리.
- 레이어 미선택 시 도형 경로, 일반 레이어 선택 시 닫힌 마스크 생성. 도형 위에는 Ctrl+펜으로 마스크 생성. 기존 도형 경로/마스크는 점이나 선을 직접 클릭해 편집.
- 여러 경로 마스크의 Add/Subtract/Intersect/None, 반전, 순서 변경, 제거. Properties 안에서 조작하며, 일반 레이어와 Adjustment가 같은 마스크 평가를 사용.
- 프로젝트 v29 저장/읽기, 기존 사각형 마스크 호환, 부모 변환을 포함한 레이어 좌표 편집, 문서/시간/선택 변경 시 오래된 제스처 취소.
- 당시 미구현(경로/마스크 키프레임과 Feather/Expansion/Opacity는 위 후속 단계에서 구현): 로토베지어, 점 다중 선택·수치 편집, 경로 도형의 자동 경계·피킹 정교화(현재 소스 크기는 컴포지션 크기), 도형 Contents 그룹·연산. 이 단계는 정적 경로 편집의 기반이며 E02/G01 전체 완료가 아님.

검증: 코어 126개 + 데스크톱 127개 테스트 통과(외부 미디어/장치 테스트 30개 제외), Cargo check/fmt/release build 통과. Moon/proto가 PATH에 없어 Cargo로 직접 실행했다. 실제 창에서 G, 곡선 드래그·닫기·채우기, 점 이동 Undo/Redo, Solid 위 Enter 마스크 완료, 두 경로 Add/Subtract 조합·연산 Undo, v29 저장을 확인했다. 저장 파일의 CLI PNG 출력(1920×1080)에서 곡선 내부와 마스크 내부는 불투명, Subtract 구멍은 알파 0을 확인했다. 단일 편집기 교체 후 실행 창은 1개이며 원래 미저장 작업은 Desktop의 `editing-preserved-before-paths-20261002.lfe.json`에 보존했다.

### 편집 작업 공간 우선 개발 — 2026-10-02

실제 실행 중인 After Effects 2026의 기본 작업 공간을 확인했다. 상단은 편집 도구, 왼쪽은 Project/Effect Controls, 중앙은 Composition, 하단은 Timeline, 오른쪽은 접이식 Properties/Info/Audio/Preview/Effects & Presets/Character/Paragraph와 Align으로 정리한다. 고급 렌더링 확장은 보류한다.

이번 구현 범위:
- 선택/Hand/Zoom/회전/Anchor/도형/Text 도구 모음, 도형 선택 메뉴와 Q 순환, Snapping. Save/Undo/Redo/Render Queue는 기존 메뉴·단축키에서 접근.
- 새 컴포지션 시작 카드, Ctrl+N 생성 설정, Ctrl+Alt+N 새 프로젝트, Ctrl+K 기존 컴포지션 설정. Footage 카드로 가져오기와 소스 기반 컴포지션을 한 이력으로 생성.
- Rectangle/Rounded Rectangle/Ellipse/Polygon/Star 드래그 생성, Shift 비율 제한, Alt 중심 기준, Escape 취소, 채우기·선 색/두께·둥글기·꼭짓점/내부 반경. 한 드래그 = 한 Undo.
- 12개 기존 효과의 검색·분류 폴더 및 적용 후 Effect Controls 표시. 사용자 효과 프리셋 저장은 아직 미구현.
- Wanted Sans Character의 글자 크기·행간·자간·색, Paragraph의 좌/중앙/우 정렬. 도형은 프로젝트 v27, 단락/자간/행간은 v28에 저장. 직접 캔버스 텍스트 편집·여러 글꼴/굵기·문자별 스타일·Text Animator는 남음.
- 타임라인 보조 작업을 아이콘으로 축소하고 중복 Snap 버튼을 제거.
- 단일 편집기 정책은 테스트용이 아니라 앱 시작 시 항상 적용. 새 실행은 기존 창의 활성 필드를 확정하고 재생/진행 작업을 중단·정리한 뒤, 미저장 프로젝트를 복구 사본에 보존하고 기존 창을 닫는다. 후속 실행은 잠금 해제 후 작업 창을 연다. 동시 후속 실행은 하나만 대기하며, 60초 이내 종료하지 못하면 두 번째 창은 열지 않는다. CLI·FFmpeg 자식 작업은 제한 대상이 아니다.

다음 편집 기능(완료로 취급하지 않음): 캔버스 텍스트 편집·글꼴/굵기와 텍스트 선택, 실제 색 선택기·도형 도구의 Fill/Stroke 기본값, 도형 크기/파라미터 키프레임·Contents 그룹/연산, 효과 프리셋·추가 기본 효과, 속도 그래프·공간 경로, 검색/키보드 탐색·패널 크기/DPI 회귀. Brush/Clone/Eraser/Puppet·Tracker·3D 도구는 별도 구현이 필요하며 작동하지 않는 버튼을 지원 기능으로 표시하지 않는다.

검증: 코어 124개 + 데스크톱 122개 테스트 통과(미디어/오디오 외부 환경 테스트 30개는 이번 실행에서 제외), Cargo check/fmt/release build 통과. 네이티브에서 새 컴포지션, 드래그 사각형, 한 단계 Undo/Redo, Ctrl+T 텍스트, 색 입력·중앙 정렬, Gaussian Blur 적용/Effect Controls 자동 전환, 저장(v28)·PNG 출력, 960×540 푸티지 시작 카드의 단일 컴포지션 생성을 확인했다. 동일 실행 파일 재실행 및 다른 경로/이름 실행 모두 이전 PID 종료와 새 PID 하나만 남는 것을 확인했다. 미저장 도형과 아직 Enter를 누르지 않은 색상 FF8844도 새 세션에서 복구했다. 마지막 효과 패널 높이 조정 이후 UI 자동화의 `failed to activate captured window` 오류로 최종 패널의 화면 재검증은 완료하지 못했다. 전체 AE 동등 구현으로 판단하지 않는다.

기존 문서 보존: 10월 1일 사본에 덮어쓰지 않고 현재 미저장 작업을 바탕 화면의 `lower-third-preserved-20261002-editing.lfe.json`에 별도로 저장했다.

| 항목 | 구현한 범위 | 남은 범위 |
| --- | --- | --- |
| A01 | 안정적인 컴포지션 ID, 생성·복제·삭제·전환, Project 검색 목록과 뷰어 탭, 독립 편집·Undo/Redo·저장·CLI 출력, 참조 중 삭제 방지 | 최대 100개 컴포지션·전체 1,000 레이어 |
| A02 | 공유 자산 ID·폴더 트리·이름/유형 검색과 정렬·실제 이미지/영상/컴포지션 썸네일·메타데이터/참조 수, 소스 이름/폴더 편집·재사용 레이어·공유 재연결·Undo/Redo·버전 22 저장·이전 파일 변환 | 미디어 1,000개·폴더 1,000개·32단계; 폴더 펼침/검색/정렬은 현재 세션 상태; 다중 선택/드래그 이동·키보드 트리 탐색은 후속 UX 작업 |
| A03 | 연속 레이어의 모든 속성 이동 방식 Pre-compose, 소스 추가·열기, 중첩 렌더·알파·유리수 FPS 변환, 순환/누락 참조 거부, 저장·Undo/Redo; 인스턴스별 Time Remap은 D05 | 외부 부모 관계·불연속 선택은 보존을 위해 제한; 변환 축소·속성 남기기 모드 미구현 |
| A04 | 인스턴스별 잠금 슬롯, 이전 체크포인트, 복구 선택·보류, 문서 전환 후 오래된 쓰기 차단 | 긴 기간의 버전별 백업 정책과 프로젝트별 복구 관리 화면 |
| A05 | 이미지 자산을 파일에 한 번 저장, Undo/복제 간 메모리 공유, 파일 한도 확대, 고유 이미지 총량 제한 | 외부 자산 패키지와 모든 메타데이터 편집의 저장 크기 사전 점검 |
| A06 | 프로젝트 폴더 내부 상대 경로·Save As 재기준화, Collect Files의 영상 중복 제거·취소·실패 정리, 누락/참조 수 목록·개별 및 폴더 일괄 재연결, 전체 컴포지션 공유 갱신·Undo/Redo·버전 15 | 폴더 밖 소스는 절대 경로, 수집 사본 전체를 이동; 검색 10,000개·16단계·동명은 수동 선택; 내장 이미지 한도 유지 |
| A07 | 유리수 FPS·NTSC 입력·시작 NDF 타임코드, 초/프레임/타임코드 길이·HD/UHD/길이 프리셋, 32MP 설정 검사, 공통 중첩/복사/마커 시간 변환·버전 14 저장·MP4/MOV 정확한 시간 기준 | 1–240fps·24시간 길이; 시작은 표시 오프셋, FPS 변경은 프레임 번호 보존; 드롭 프레임 번호 미지원 |
| A08 | Ctrl+I의 PNG/JPEG/영상 혼합 다중 가져오기·단일 Undo·실패 시 전체 거부, 소스 중복 제거·폴더 지정; 공유 소스 FPS/알파 해석·Straight/Ignore/Premultiplied 매트 색·반전·Reset, 소스 크기/FPS/길이 기반 컴포지션 생성; PNG/JPEG 번호 시퀀스·Error/Hold/Transparent·폴더/프레임 재연결·상대 경로/수집·공유 매니페스트·Undo/Redo·버전 24 저장 | 시퀀스 100,000프레임·경로 8MiB·이미지 축 4096px; TIFF/EXR·폴더 감시/범위 자동 확장·사용자 패턴 미구현; 영상 오디오는 H01–H04; 해석 변경은 기존 레이어 트림/키 시간 보존, 1–240fps·24시간·8비트 sRGB; 자동 추정·필드 순서·픽셀 종횡비·ICC 해석 미구현 |
| H01 | 독립 오디오 자산·영상의 첫 오디오 스트림, 샘플레이트/채널/레이아웃/길이/시작 오프셋, 타임라인 파형·소스 썸네일, 트림/이동/분할/속도/Time Remap·공유 재연결·미디어 수집·버전 25 저장 | 첫 스트림만 선택; 8–384kHz·1–32채널·24시간, 파형은 최대 48kHz 디코딩·100 bins/s·10초 청크·40MiB 캐시·행당 60청크까지; 긴 구간은 확대 안내; 전체 길이 개요·스트림 선택 남음; Windows 장치 재생은 H02, 출력은 H04의 아래 범위 구현 |
| H02 | Windows WASAPI 기본 공유 출력·48kHz 스테레오, 장치 시계 기반 플레이헤드·유리수 반복/구간 끝 정지, 취소/일시정지/탐색·75ms 디바운스 후 100ms 스크럽, 버퍼링·오류·재생 블록 Peak/RMS/클리핑 표시 | 장치 선택·기본 장치 변경 자동 복구·비Windows 백엔드·설정 기억 남음; 초기 0.5초 프리롤·100ms 장치 버퍼 요청, 디코더/합성/디스플레이 실제 지연 개선은 J01/J02; 미터는 완료한 최대 100ms 블록, True Peak/피크 홀드 미구현 |
| H03 | 독립/영상/프리컴프 오디오 On/Off, 좌우 -192…+12dB·스테레오 Pan·Fade 공통 키프레임/값 그래프·0.5초 페이드 생성, 레이어 Solo 연계, 중첩 버스 행렬·부동소수점 합산·최종 클램프, 최대 100ms 믹스의 Peak/RMS/클리핑 측정, 버전 26 저장 | 팬은 반대 채널을 sin/cos로 이동하고 끝에서 합산하므로 위상 상쇄·상관 소스 증폭 가능; 페이드는 진폭 선형·겹치는 구간의 Fade 키 대체, 짧은 레이어에 맞춰 축소; 미터는 버튼으로 측정한 구간 결과, 재생 블록 미터·Windows 장치 연결은 H02에 구현; 피크 홀드 남음; 오디오 효과/EQ/컴프레서는 후속 범위 |
| H04 | 48kHz 스테레오 공통 믹서·트림/분할/이동·역재생/정지·영상 시작 오프셋·중첩 Time Remap, AAC 192kbps MP4·24비트 PCM MOV, 출력 모듈/CLI Audio auto/off·큐 v3 무음 설정 변환, 취소/실패 원본 보존 | 속도에 따라 피치 변경; 정지/범위 밖·출력 FPS 올림 꼬리는 무음; 48kHz PCM 선형 보간으로 고속 재생 aliasing 가능, 가변 비율 대역 제한 리샘플러/피치 보존·채널/레이트 옵션 남음; Windows 장치 시계 검증은 H02; 실제 영상 표시 지연·타 플랫폼 장치 검증 남음 |
| I01 | 원본·프로젝트 출력 경로 보호와 하드링크 검사, 누락 영상·출력 크기 사전 검사, 설정 크기 제한 | 설치된 인코더/글꼴을 한 화면에서 점검하는 UI와 디스크 여유 용량 안내 |
| I02 | 타임라인 도크 Render Queue·Ctrl+M 스냅샷/워크 영역 등록, 작업 순서·활성화·범위·출력 경로·재시도, MP4/MOV/PNG 복수 출력 모듈·형식 묶음 프리셋, 순차 실행·Stop/Continue 실패 정책·취소·로컬 Undo/Redo·재시작 상태 복원 | 100작업·작업당 8출력·32프리셋·스냅샷 1GiB; 소스 파일 바이트는 외부 참조, 큐는 사용자 로컬 저장; 실행 시작 시 큐 편집 이력 초기화; AAC/PCM auto/off 연계 완료; 추가 영상/오디오 프로필은 I03 |
| I03 | 출력 모듈별 크기·유리수 FPS·RGB/RGBA/Alpha 채널·H.264 CRF/목표 평균 비트레이트/속도, ProRes 4444·PNG 형식 선택, 설정 포함 프리셋·큐 버전 2 변환·Undo/Redo·CLI 공통 설정 | AAC/PCM auto/off 연계; 추가 오디오 레이트/채널과 코덱 프로필 남음; FPS 변환은 Hold 반복/생략, 마지막 프레임 올림; 8비트 sRGB·32MP·축 16384·1–240fps·명시 MP4 크기는 짝수, 크기 종횡비 변경은 Stretch; ABR은 목표치이며 CBR 보장 없음; 단일 File 빠른 출력은 기존 기본값 |
| I04 | sRGB 전달 함수·BT.709 행렬/원색·제한 범위 출력 정책, MP4/MOV 태그·픽셀 왕복 검사 | ICC 입력·디스플레이 프로파일·HDR·선형 합성 |
| J01 | 미리보기/출력 공통 지속 CFR 디코더·4세션 LRU·세션당 최대 2프레임/8MiB 프리페치와 단일 읽기 버퍼, 렌더러당 120프레임/32MiB PNG 캐시, 인접 32프레임 전진 재사용·파일 메타데이터 무효화·Refresh, 모든 미리보기의 비동기 합성·단일 작업·탐색/반복/문서/품질 세대 검사·취소/자식 종료 | 멀거나 캐시 없는 역방향 탐색은 재시작, VFR 미지원; PNG/SVG 경로 유지, 단일 SVG 파싱/래스터 패스는 중간 중단 불가; 전체 합성 RAM/디스크 캐시와 표시 지연은 J02/J03; FFmpeg 자체 메모리는 별도 |
| J02 | 합성 RGBA RAM 캐시·64/256/512MiB/Off·4096프레임 한도·LRU, 작업 영역 사전 캐시/중단/초기화·상주 구간 초록색 표시·메모리/히트 통계, 편집/Undo/문서/품질/Refresh 무효화와 백그라운드 파일 size/mtime 감시 | RAM 부분 구현; 디스크 캐시·설정 영속화·실제 표시 지연/장시간 FPS 측정 남음; 메모리 한도는 RGBA 캐시만 적용, 같은 size/mtime 교체는 Refresh 필요 |
| L01 | Moon desktop 테스트/format 태스크, Windows 전체 테스트·FFmpeg·릴리스 빌드 CI 정의 | 원격 CI 실제 실행 및 DPI/UI 자동화 커버리지 |
| L02 | 사용자별 단일 작업 창·새 실행 시 기존 창 교체·미저장 복구 사본·동시 시작 게이트·충돌 후 잠금 해제 | 설치/업데이트·배포·진단 |
| A09 | B/N 워크 영역과 별도 편집기 메타데이터, 컴포지션별 재생 위치·시간축/미리보기 줌·팬·품질·그래프, 패널 비율·열 너비·탭 저장/복원 | 화면 변경은 Undo/dirty에 포함하지 않고 명시적 Save로 보존; 복구 체크포인트는 기본 화면으로 시작 |
| C01 | Null 변형 컨트롤러, 독립 Solid 크기·색·컴포지션 크기 복사, Adjustment의 아래 합성 결과·변형/마스크·불투명도·효과 순서/애니메이션, Undo/Redo·버전 16 | 소스 크기 편집은 원점·앵커·키 유지; 조정 경계는 8비트 sRGB 래스터, 프리컴프 시 하위 입력 전부 포함 |
| C02 | Solo·Shy·Guide 스위치, Hide Shy 목록 필터, 미리보기/중첩/출력별 Guide 처리, 저장·Undo/Redo | 레이블 편집·품질 스위치; 블렌딩은 G03에서 구현 |
| C03 | 레이어 Copy/Cut/Paste, 프로젝트 안 컴포지션 간 복사, 새 ID·부모 재연결·FPS 시간 변환, 충돌 시 원자적 거부 | 프로젝트 간/OS 클립보드, 플레이헤드 기준 레이어 붙여넣기 |
| C04 | 컴포지션·레이어 마커의 이름·색·기간 편집, 이전/다음 탐색, 이동·분할·중첩·FPS 복사, Undo/Redo·버전 13 저장·재열기 | 한 소유자당 1,000개; 분할/FPS 변환 시 시작 시각 충돌이나 기간 소실은 원자적으로 거부 |
| C05 | 재생 위치·워크 영역·레이어 경계·변형/효과 키·마커 시작/끝 스냅, 다중 선택 간격 보존, 8 논리 픽셀 허용거리, Alt 임시 해제·Snap 토글 | 구현 범위의 확대 배율·입력 회귀 검증 유지 |
| C07 | 다중 선택 회전·스케일 드래그, Composition/Selection 6방향 정렬·6방향 분배, Pick Whip 부모 연결·현재 포즈 보존, 부모/자식 이중 변형 방지·Undo/Redo·저장 | 각 루트 앵커 기준; 소스 경계 사용·마스크/효과 확장 제외; 수치 필드/앵커 도구는 단일 레이어; 0 스케일 축은 퍼센트 포인트 증분 |
| B09 | 눈금자·가이드 생성/이동/제거/잠금·그리드·8px 스냅·90/80% 안전 영역, RGB/개별 채널/알파 보기·좌표/RGBA/샘플 해상도 Info, 가이드 Undo/Redo·버전 20 및 컴포지션별 뷰 복원 | 가이드 최대 256개·±32768px; 그리드 메뉴 50/100/200px; 소스 경계·중심 이동 스냅; 사용자 눈금자 원점·가이드 프리셋 교환 미지원 |
| B03 | 왼쪽 Effect Controls, 오른쪽 분류 폴더/검색·효과 적용 후 Controls 전환, Effect 메뉴와 선택 레이어 연결 | 사용자 프리셋 저장·검색, 드래그 적용·키보드 목록 탐색 |
| E01/E03 | 5종 도형의 드래그 생성, Shift/Alt/Escape, 채우기·선 색/두께·둥글기·꼭짓점/별 반경, Undo/Redo·v27·공통 렌더 | 선/Pen, 도형 크기 직접 편집·그룹/연산, 도구 기본 스타일, 파라미터 애니메이션 |
| F01/F03 | Ctrl+T 텍스트 생성, Wanted Sans 크기·행간·자간·색과 좌/중앙/우 정렬, Character/Paragraph 패널·v28·공통 렌더 | 캔버스 직접 편집, Point/Paragraph 유형·자동 줄바꿈, 글꼴/굵기·커닝·문자별 스타일·Stroke/Text Animator |
| D01 | 변형/효과 공통 속성 주소, 기본값·범위·애니메이션 트랙·버전 12 저장·그래프 통합 | 색·마스크·경로·텍스트 속성 확장 |
| D05 | 영상·이미지 시퀀스·프리컴포지션의 소스 초 트랙, 가속/감속·Hold·역재생·현재 프레임 정지, Timeline/Properties/값 그래프·공통 키 편집·Ctrl+Alt+T·Undo/Redo·버전 21 저장 | 활성화 시 첫/마지막 표시 프레임 키·기존 속도 보존, 키 범위 밖은 끝값 유지·소스 범위 밖은 투명; 해제하면 기존 기본 시간 복원; 오디오는 H01–H04, 프레임 블렌딩/Optical Flow는 G07에서 별도 개발 |
| G02 | Alpha/Luma·각 반전, 순서 독립 소스·재사용·16단계 참조, 숨긴 소스 직접 변형, 타임라인/Properties 메뉴, 복제·붙여넣기·분할·프리컴프·Undo/Redo·버전 18 | 소스는 픽셀 레이어(Null/Adjustment/Audio 제외); Luma는 알파 포함 sRGB 가중값, 조정 대상은 효과 영역 제한; 참조 연결 전체 보존 정책 |
| G03 | Normal/Multiply/Screen/Add/Overlay, 타임라인 Mode·Properties 메뉴·키보드 선택, 반투명 알파·Adjustment 합성, Undo/Redo·버전 17 | 8비트 sRGB 합성, Add는 색 채널 합산 후 제한; 프리컴프는 하위 입력 전체 포함 |
| G04 | 효과 추가·삭제·복제·이름·순서·우회·Reset, 파라미터 키프레임·보간·탐색, 기존 효과 무손실 변환, 타임라인·그래프·일괄 키 선택/이동/복사/삭제 | 구현 범위의 추가 회귀 검증 유지 |
| G05 | Fill/Tint·Levels·Hue/Saturation·Glow·Drop Shadow·Curves·Linear/Radial Gradient와 순차 합성, RGB/채널별 5점 곡선·그래프·파라미터 키·버전 19 | 곡선 자유 점/Pencil·ACV/AMP, Gradient 디더링·캔버스 끝점 핸들, 효과별 추가 고급 옵션 |

현재 구현은 로컬에서 `cargo check --workspace`, `cargo fmt --all --check`, 기본 테스트 238개와 FFmpeg 통합 테스트 29개, 별도 Windows 오디오 장치 테스트 1개로 검증했다. 통합 테스트에는 단일/중첩 장면의 MP4·알파 MOV 왕복 검사가 포함된다. 기존 샘플의 프리컴포즈·분할·저장·재열기 후 렌더 픽셀 일치, 새 네이티브 앱의 생성·복제·소스 편집·부모 반영·저장도 확인했다. `examples/precomposition-study.lfe.json`은 중첩 타이틀 예제다. 이번 레이어 작업은 Null·스위치·클립보드·FPS 변환·저장 및 Guide 출력 제외 회귀 검사를 포함한다. 최종 릴리스로 `examples/layer-workflow.lfe.json`의 0/149프레임 PNG와 MP4를 출력하고, 부모 애니메이션·Guide 제외·MP4 배경색을 픽셀 값으로 확인했다. 네이티브 앱에서 Null 생성·속성 및 Guide 미리보기를 확인했고, 효과 스택 작업에서 키보드 Copy/Cut/Paste·다른 컴포지션 붙여넣기·Undo/Redo·저장도 검증했다. 효과 복제 시 파라미터 키 두 개가 함께 복제되며, 저장된 두 컴포지션에서 효과와 키가 유지되는 것을 확인했다. `examples/effect-study.lfe.json`은 Blur·Drop Shadow·Glow를 사용하는 애니메이션 예제이며 최종 릴리스로 MP4와 PNG를 출력했다. 기존 효과 변환은 마스크와 넓은 Blur에서도 픽셀 일치 검사를 통과했다. 공통 키 경로는 변형/효과 혼합 이동·복사·삭제·저장, 그래프 값/시간·범위 거부, 효과 삭제 후 선택 정리 회귀 검사로 검증했다. 네이티브 타임라인에서 효과 키를 30→45프레임으로 드래그하고 그래프에서 60프레임·20.19 값으로 편집한 뒤 저장된 JSON의 값을 확인했다. 그래프 이동 후 Copy가 레이어 복사로 바뀌지 않도록 키 선택도 새 주소로 유지한다. 마커 추가·변경·이동·분할·중첩·FPS 변환·잠금·범위 오류·Undo/Redo·파일 왕복과 미리보기/출력 픽셀 불변을 검사했다. 스냅은 확대 시 반올림 전 포인터 거리로 판단하며, 네이티브 최종 빌드에서 같은 위치의 드래그가 ON일 때 마커 45프레임, OFF일 때 46프레임으로 도달함을 확인했다. 네이티브 마커 이름 편집·레이어 마커 생성·저장·재열기 및 저장 파일의 PNG 출력도 확인했다. 화면 상태는 컴포지션 전환·삭제 Undo/Redo·저장/재열기·범위 축소·기존/손상/미래 메타데이터 검사로 검증했다. 네이티브 앱에서 60프레임·시간축 2배·미리보기 100%·팬 (73, 36)·프로젝트 패널 비율 0.25537을 저장하고 New 이후 다시 열어 복원했다. 메타데이터 포함 파일의 60프레임 CLI PNG는 변경 전 출력과 SHA-256이 일치했다. 유리수 FPS는 시간 경계·반올림·음수 소스 시작·마커/키 복사·중첩 샘플·기존 파일·Undo/Redo·범위 거부로 검증했다. 24000/1001·30000/1001 MP4/MOV 각각의 프레임 수·시간 기준·타임코드·합성 픽셀을 FFprobe/FFmpeg로 확인했다. 네이티브 앱에서 29.97fps·10초·시작 01:00:00:00 입력, Undo/Redo·저장·재열기를 확인했고, 저장 파일의 CLI 출력은 30프레임·1.001초·01:00:00:02 타임코드였다. 미디어 경로는 상대 저장·Save As·한글 폴더 이동·누락 상태 열기·공유 소스 재연결·실패 원자성·복사 취소·동명 검색 거부로 검증했다. 통합 검사에서는 원본 영상을 제거하고 수집 폴더를 옮긴 후 중첩 PNG 픽셀 일치와 MP4 출력을 확인했다. 네이티브 앱에서도 두 컴포지션의 누락 소스 하나를 폴더 검색으로 연결하고 Undo/Redo·상대 경로 저장·Collect Files를 실행했다. 수집 폴더 이동 후 재열기와 미리보기, 원본/수집본의 PNG SHA-256 일치, 30000/1001fps MP4 출력도 확인했다. Solid 복제 간 독립 크기·색 편집, 잠금·범위 거부·Undo/Redo·버전 16 왕복을 검사했다. Adjustment는 반투명 알파 유지·효과 순서·블러·마스크/반전·변형·시간 범위·애니메이션·상위 레이어 제외·중첩 및 PNG/MP4/MOV 픽셀 검사를 통과했다. 네이티브 Ctrl+Y/Ctrl+Alt+Y 생성, 640→480px Solid 편집, Grayscale·마스크 적용, Undo/Redo·저장·재열기를 확인했다. 저장한 프로젝트의 CLI PNG는 마스크 밖 [255,0,0,128], 안쪽 [54,54,54,128]이며 투명도는 유지된다. `examples/adjustment-study.lfe.json`의 0/45프레임 PNG와 30프레임 MP4를 최종 릴리스로 출력했다. 블렌딩 다섯 모드는 불투명/반투명 기준 픽셀·투명 경계·마스크·시간 범위·조정 레이어 알파·프리컴프 일치·저장 왕복 및 MP4/MOV 픽셀 검사로 검증했다. 네이티브 Mode 메뉴의 마우스 변경·Undo/Redo·Screen 저장 복원·방향키/Enter/Space/Escape 조작과 Add 저장을 확인했다. `examples/blend-modes-study.lfe.json`의 PNG 0/45프레임과 MP4 30프레임을 출력했다. Track Matte는 네 가지 모드의 기준 픽셀·마스크·블러/Fill·시간 범위·불투명도 애니메이션·중첩/공유·부모 변형·Adjustment 영역과 숨긴 오프라인 영상 사전 검사, MP4/MOV 왕복 검사를 통과했다. 네이티브 소스 지정·자동 숨김·키보드 Luma 반전·숨긴 소스 드래그·Undo/Redo·타임라인 열 정렬·저장/재열기를 확인했다. 저장된 소스 위치는 (568.0435, 314.0217)이며 CLI PNG 알파는 매트 밖 204, 안쪽 153이다. `examples/track-matte-study.lfe.json`의 0/45/89프레임 PNG와 30프레임 MP4를 릴리스로 출력했다. Curves는 전체 256단계 입력의 identity/반전·마스터와 채널 순서·알파, Gradient는 선형/방사형 위치·원본 혼합·축소 반경·애니메이션·마스크·Adjustment·중첩 및 PNG/MP4/MOV 왕복을 검사했다. 네이티브 곡선 드래그·Shift 방향키·Undo/Redo·채널 전환·0/45프레임 키, Gradient 끝점 (640, 360)·원본 혼합 50% 편집·버전 19 저장/재열기를 확인했다. 재열기 후 CLI PNG 중심은 0프레임 [96,161,159,255], 45프레임 [96,115,160,255]이며 바깥 알파는 0이다. `examples/tonal-color-study.lfe.json`의 PNG 0/45/89프레임과 960×540·30fps·30프레임 MP4를 릴리스로 출력했다. 다중 변형은 부모/자식 선택·음수/0 스케일·애니메이션 키·회전된 부모 좌표·잠금/범위 오류 원자성·Undo/Redo·저장 왕복을 검사했다. 네이티브에서 중심 X=(200,460,720), Y=290으로 분배/정렬하고 세 레이어를 함께 88.78668° 회전, (151.11605%,148.89301%) 스케일로 편집했다. Pick Whip 연결·Undo/Redo·재열기 후 부모 관계와 Align Selection 설정이 유지됐으며, 연결 전후 PNG SHA-256이 일치했다. 저장 파일의 960×540·30fps·30프레임 MP4에서 배경색과 세 레이어 색을 픽셀 값으로 확인했다. 뷰어 도구는 가이드 버전·범위·Undo/Redo·컴포지션 복제/신규, 채널별 RGBA 변환·샘플 좌표, 줌별 스냅/Alt 해제, 눈금자를 제외한 Fit 및 PNG/미리보기 픽셀 불변 검사를 통과했다. 네이티브에서 가이드 X=479/Y=271을 만들고 Undo/Redo·이동·제거·복원, 레이어 중심 X=479 스냅, 키보드 Alpha 선택과 재열기를 확인했다. 최종 릴리스의 Full/Half Info는 반투명 픽셀 [82,122,255,128]과 960×540/480×270 샘플 해상도를 표시했다. 가이드·그리드·Alpha 설정 저장 전후 PNG 해시가 일치하고 MP4의 가이드 좌표에는 배경색만 출력된다. 원격 CI 실행과 AE 전체 기능 동등성은 아직 검증하지 않았다.

Time Remap은 영상의 소스 시작·음수 원점·속도/역재생/정지 보존, 24000/1001·30000/1001 FPS, 중첩·프리컴프·분할·이동·키 복사/FPS 변환, 잘못된 입력의 원자적 거부와 버전 21 왕복으로 검증했다. 변형·효과는 컴포지션 시간을 유지하고 소스만 다시 매핑한다. PNG/미리보기 픽셀, 리맵 영상과 중첩 소스의 60프레임 MP4·알파 MOV 색/투명도/배경 합성 검사를 통과했다. 네이티브에서 75프레임에 소스 시간 1.25초를 입력하고 Undo/Redo, 그래프에서 90프레임·2.070433333333초로 드래그, 리맵 해제/Undo와 현재 소스 프레임 정지를 확인했다. 저장·다른 문서 열기·재열기 후 값과 미리보기가 유지됐으며 저장 파일의 90프레임 CLI PNG도 출력했다. `examples/time-remap-study.lfe.json`은 하나의 소스를 원속도·가속/감속·Hold·역재생으로 재사용하는 예제이며, 릴리스에서 960×540·30fps·180프레임 MP4를 출력했다.

2026년 10월 2일 자산 관리 검증: 공유 소스의 레이어 간 재사용·폴더 순환/누락 거부·미사용 자산 보존·이전 파일 변환·잘못된 가져오기 전체 취소·영상 재연결/수집·저장 왕복을 검사했다. FFmpeg 통합 테스트에서는 PNG/영상 혼합 가져오기, 두 컴포지션의 소스 재사용과 재연결, MP4/MOV의 색·알파·배경색을 확인했다. 네이티브에서 소스 이름 변경, 폴더 이동 Undo/Redo, 자산에서 레이어 추가, PNG/MP4 동시 가져오기와 단일 Undo/Redo, 접힌 폴더 내부 검색을 확인했다. 저장 파일을 다시 열어 자산 3개·폴더 2개·컴포지션 2개와 영상 미리보기가 복원됐고, 같은 파일의 PNG 및 1280×720·30fps·30프레임 MP4를 최종 릴리스로 출력했다. `examples/asset-library-study.lfe.json`은 두 컴포지션이 한 내장 이미지를 재사용하는 독립 실행 가능한 예제다. Project/Composition/Timeline/우측 도크의 기존 경계를 유지했다.

2026년 10월 2일 소스 해석 검증: FPS 변경의 공유/잠금 인스턴스 갱신·트림/키 보존·재연결, 역재생/정지/Time Remap·컴포지션 간 복사, 유리수 FPS와 버전 23 저장을 검사했다. Straight/Ignore/Premultiplied의 검정/흰색 매트·반전·효과·축소·썸네일 및 실제 영상 디코딩/MP4/MOV 픽셀을 검증했다. 네이티브에서 30fps·6초 소스를 15fps·12초로 해석하고 960×540·180프레임 컴포지션 생성 Undo/Redo를 확인했다. 반투명 이미지의 Premultiplied 적용·Undo/Redo·저장·재열기도 확인했다. 저장 파일의 PNG 픽셀은 [199,100,50,128], 알파 MOV는 [199,100,50,129], 검정 배경 MP4는 [100,49,25,255]였고 두 영상은 15fps·15프레임이었다. 해석한 영상 컴포지션의 PNG와 960×540·15fps MP4도 출력했다. 기존 패널 경계를 유지했다. 이후 이미지 시퀀스 구현과 검증은 다음 단락에 기록한다.

2026년 10월 2일 이미지 시퀀스 검증: 번호 정렬·누락 슬롯 보존·손상/크기 불일치의 전체 가져오기 거부, FPS/알파 해석·역재생/정지/Time Remap·분할/이동/컴포지션 간 복사, 공유 매니페스트·버전 24 저장을 검사했다. 폴더/개별 재연결·Undo/Redo·상대 경로·Save As·Collect Files의 누락 슬롯 보존·수집 폴더 이동·원본 경로 보호도 검증했다. 네이티브에서 0002번이 없는 PNG 시퀀스를 가져오고 Undo/Redo, 2fps·3프레임 컴포지션 생성, Error/Hold/Transparent 전환, 폴더 재연결 Undo/Redo와 저장/재열기를 확인했다. 저장한 프로젝트의 PNG는 첫 프레임 [0,199,100,128], 누락 프레임 [0,0,0,0], 마지막 프레임 [239,100,20,128]이었다. MP4와 알파 MOV 모두 320×180·2fps·3프레임·1.5초이며, MP4는 검정 배경 합성, MOV는 알파 보존을 확인했다. 같은 편집기 한 개로 검증했고 기존 사용자 문서는 변경하지 않았다.

출력 사전 검사 보완: Time Remap이 있는 중첩 컴포지션은 실제로 샘플링되는 소스 프레임만 검사한다. 누락된 시퀀스 프레임을 건너뛰는 리맵은 통과하고 해당 프레임을 참조하면 실패하는 회귀 검사를 추가했다. 일반 중첩 FPS 변환의 검사는 여전히 연속 범위를 보수적으로 검사한다. 모든 메타데이터 편집의 저장 크기 사전 점검은 A05에 남아 있다.

렌더 큐 검증: 스냅샷 격리·서로 다른 컴포지션의 순차 출력·순서 변경·다중 출력·형식 프리셋·범위·독립 Undo/Redo·재시작 복원, Stop/Continue 실패 정책·재시도·실행 중 취소·중단 상태 복원을 검사했다. 원본/하드링크/큐 저장소 경로 보호, 메타데이터 쓰기 실패, PNG 임시 디렉터리 정리도 검증했다. 최종 네이티브 앱에서 Ctrl+M 등록, MP4/MOV/PNG 출력 추가, Delivery QA 프리셋 저장·재시작 복원, 방향키/Enter 프리셋 선택, 범위 변경 Undo/Redo를 확인했다. 빈 프로젝트를 연 상태에서 저장된 스냅샷 작업을 실행해 세 출력이 모두 Done이 되었다. 영상은 320×180·2fps·3프레임·1.5초였고, 세 형식의 모든 디코딩 픽셀이 같은 프로젝트의 직접 출력과 일치했다. MP4는 배경 합성, MOV/PNG는 누락 프레임의 투명도와 반투명 소스 알파를 유지했다. 완료 후 다시 시작해 Done 상태와 프리셋 복원을 확인했으며, 원래 사용자 문서의 43프레임 화면으로 복귀했다. 편집기는 한 개이고 원래 파일의 SHA-256은 변경되지 않았다.

출력 설정 검증: 유리수 FPS 변환의 프레임 수·구간 경계·속도 유지, 확대/축소와 채널별 픽셀, 잘못된 크기/FPS/코덱 조합의 사전 거부, 설정 프리셋·Undo/Redo·재시작 복원·버전 1 큐 변환, CLI 옵션과 실패 시 기존 파일 보존을 검사했다. FFmpeg에서 H.264/ProRes의 크기·FPS·길이·프레임 수·알파 유무와 픽셀을 검사했고, H.264 비트스트림의 CRF 30 및 목표 700kbps도 확인했다. 네이티브에서 160×90·4fps·CRF 28·fast 편집과 크기 Undo/Redo, MP4 RGBA 거부, Scaled delivery QA 프리셋 저장·재시작 선택·설정 복원·렌더를 확인했다. 생성 MP4는 6프레임·1.5초이며 동일 CLI 출력과 전체 디코딩 픽셀이 일치했다. CLI 알파 전용 PNG의 중심은 [128,128,128,255]였다. 최종 릴리스의 PNG는 실제 gray 8비트이며 MOV RGB/RGBA는 각각 yuv444p12le/yuva444p12le로 디코딩되어 채널 저장 방식도 구분된다. FPS 변환은 원본 프레임 Hold이며 연속 시간의 새 모션 샘플은 생성하지 않는다. 오디오는 이후 H04에서 연속 샘플 시간으로 추가했다. 원래 사용자 문서의 43프레임 화면으로 복귀했고 파일 해시는 동일하며 편집기는 한 개다.

오디오 자산·파형 검증: 48kHz 스테레오 PCM의 반대 위상·무음·10초 청크 경계와 44.1kHz FLAC/MP3/AAC 디코딩, 영상보다 0.5초 늦은 오디오 시작을 검사했다. 트림/이동/분할·Undo/Redo·속도/역재생/연속 소스 시간·Time Remap·FPS 해석, 공유 재연결·잘못된 메타데이터의 원자적 거부, 이미지/시퀀스와 버전 25 저장·미디어 수집을 확인했다. 네이티브에서 WAV/MOV 동시 가져오기, 6초 오디오 컴포지션 생성, 2초 분할 Undo/Redo, 첫 구간 15프레임 이동 후 30–75프레임 트림, 두 인스턴스 공유 재연결 Undo/Redo·저장·재열기를 확인했다. 영상 파형은 독립 소스보다 0.5초 늦게 시작했다. 저장 파일의 CLI PNG는 오디오를 제거한 비교 프로젝트와 바이트 단위로 같았고 MP4는 320×180·30fps·30프레임·1초의 무음 영상이었다. 오디오를 영상용 Track Matte의 소스/대상으로 지정하는 명령은 문서를 변경하지 않고 거부한다. 이 H01 검증 당시 소리 재생/믹싱/출력은 미구현이었다. 이후 출력 믹서는 H04, 레벨·팬·페이드 편집은 H03, Windows 장치 재생은 H02에 추가했다. 원래 사용자 문서의 43프레임 화면으로 복귀했고 파일 해시는 유지했으며 편집기는 한 개다.

오디오 믹싱·출력 검증: 블록 크기와 무관한 샘플 시간, 중첩 Time Remap·Guide/Solo·시각적 숨김, 분할·Undo/Redo·저장 왕복, 위상·클리핑·워크 영역 끝 무음과 비정수 FPS 샘플 수를 검사했다. 44.1kHz WAV/FLAC/MP3/AAC의 구간 디코딩을 연속 디코딩과 비교했고 영상 오디오의 0.5초 오프셋, 취소/누락 소스의 기존 출력 보존과 임시 파일 정리를 확인했다. 네이티브에서 Audio auto/off·Undo/Redo, 320×180·0–90프레임 설정과 Audio delivery QA 프리셋 저장·재시작 복원을 확인했다. 빈 프로젝트에서 저장된 큐 스냅샷을 실행해 Done에 도달했으며, 90프레임·3초 MP4의 디코딩된 영상과 오디오가 같은 CLI 출력과 바이트 단위로 일치했다. 48kHz 스테레오 오디오의 표시 길이는 144,000샘플이다. CLI MOV의 24비트 PCM과 독립 계산한 믹스의 최대 샘플 오차는 1.20e-7 이하였다. AAC는 손실 압축이며 해당 클리핑 테스트 믹스의 MSE는 0.000261, 상관계수는 0.999684였다. Audio off에는 오디오 스트림이 없다. 구형 큐의 무음 설정 변환과 잘못된 설정의 오류 반환·원본 보존도 검사했다. 이 검증 당시 장치 재생, 레벨 편집과 고품질 가변 비율 리샘플링은 미구현이었다. 이후 레벨 편집은 H03에 추가했다. 최종 빌드에서 원래 사용자 문서의 43프레임·Role 위치 (490, 942)로 복귀했다. 편집기 프로세스는 한 개이며 보존 파일의 SHA-256은 변경되지 않았다.

오디오 컨트롤 검증: 좌우 dB·스테레오 행렬·중첩 버스·연속 시간 Fade·무음/클리핑/RMS, 공통 키 복사/이동/분할·FPS 변환·잠금/잘못된 값의 원자적 거부·버전 26 이미지/시퀀스 혼합 저장을 검사했다. 네이티브에서 왼쪽 -6.020599913279624dB와 Pan 50%, 양쪽 0.5초 페이드를 입력하고 첫 Fade 키를 15프레임/100%에서 18프레임/79.75%로 드래그했다. 값·그래프 Undo/Redo, 타임라인 음소거 시 무음 미터, 저장 후 재시작·재열기의 수치/키 복원을 확인했다. 18프레임부터 100ms 측정값은 L Peak/RMS -19.8/-22.9dBFS, R -14.6/-17.6dBFS로 독립 계산과 일치했고 클리핑은 0이었다. 저장 프로젝트의 320×180·30fps·6초 출력은 180프레임·288,000 스테레오 샘플이며 MOV PCM 최대 오차 1.19e-7, AAC MSE 1.24e-8·상관계수 0.99999894였다. PNG는 영상 전용이다. 장치 재생과 실시간 미터는 이 검증에 포함하지 않는다. 최종 빌드에서 원래 사용자 문서의 43프레임·Role 위치 (490, 942)로 복귀했고, 편집기 프로세스는 한 개이며 보존 파일 SHA-256은 변경되지 않았다.

오디오 미리보기 검증: 기본 회귀에서 유리수 FPS 반복의 샘플 나머지·24시간/32비트 이후 시계, 프리롤·장치 소비 시계·버퍼 고갈/재개·유한 구간 배출·취소를 검사했다. FFmpeg PCM의 반복/100ms 스크럽을 독립 소스 샘플 계산과 비교했다. 별도 실제 WASAPI 장치 테스트는 62초 재생에서 장치/경과 시간 최대 차이 4.789ms·버퍼 고갈 0회였고, 중지 후 위치 고정·두 위치로 탐색/재시작·반복·48,000샘플 구간 종료를 통과했다. 이는 장치 시계 검증이며 스피커 음향/실제 디스플레이 지연 측정은 아니다. 최종 네이티브 앱에서 오디오와 이동 도형을 함께 재생하며 미터·버퍼 고갈 0 표시, 일시정지 136프레임 유지, 반복 해제 후 워크 영역 마지막 149프레임 정지, 90프레임 스크럽 후 위치 고정, Audio Off의 영상 전용 재생을 확인했다. 미리보기 스위치는 프로젝트 dirty/Undo/출력에 영향을 주지 않는 세션 설정이다. 최종 빌드에서 원래 사용자 문서의 43프레임·Role 위치 (490, 942)로 복귀했다. 편집기 프로세스는 한 개이며 보존 파일의 SHA-256은 변경되지 않았다.

지속 디코더 검증: FFV1 30000/1001·B-frame H.264 24000/1001·알파 MOV의 순차/임의 탐색·Hold·역방향·반복·축소 결과가 연속 디코딩 기준 픽셀과 일치했다. 네 소스 600회 요청에서 프로세스 4개 유지·120프레임 LRU 퇴출, 파일 수정 시각 변경 시 재시작, 읽기 대기 취소·가득 찬 프리페치 종료를 검사했다. 기존 중첩/효과/마스크/알파/Time Remap/저장·출력 회귀도 통과했다. Ryzen 5 4600G·약 32GiB RAM·Windows·FFmpeg N-118651-g0e917389fe의 릴리스 측정에서 640×360 H.264 60프레임은 기존 8.843초/60프로세스에서 0.194초/1프로세스로 바뀌었다. 첫 프레임 80.806ms, 누적 pipe 대기 64.129ms·PNG/base64 46.916ms, PNG 캐시 7,824,448바이트였다. 소스 처리와 픽셀 검증 시간이며 전체 앱 FPS·표시/음향 지연·전체 메모리 측정은 아니다. 네이티브에서 영상·오디오 재생/반복·버퍼 고갈 0 표시, 166프레임 일시정지·30/90프레임 탐색, 영상 X=341.52173790341294 이동과 Undo/Redo·저장·재열기 후 위치/화면 복원을 확인했다. 저장 문서의 CLI PNG와 640×360·30fps·90프레임 MP4, 3프레임 알파 MOV를 출력했고 왼쪽 빈 영역은 PNG/MOV 알파 0, MP4 검정이며 AAC/PCM 오디오를 유지했다. 원래 사용자 문서의 43프레임·Role 위치 (490, 942)로 복귀했고 보존 파일 SHA-256은 변경되지 않았다. 편집기는 한 개이며 검증용 FFmpeg 자식 프로세스는 모두 종료됐다.


2026년 10월 2일 RAM 프리뷰 검증: RGBA 공유·알파 보존·바이트/프레임 수 제한·LRU·작업 영역 채우기·배타적 구간 끝, 편집/Undo/해상도/Refresh 무효화, 연결 파일 생성/변경/삭제와 저장 후 합성 픽셀 일치 회귀 검사를 추가했다. 네이티브 릴리스에서 640×360·180프레임을 158.2MiB로 캐시하고 반복 재생 중 캐시 히트 증가와 오디오 underrun 0을 확인했다. 64MiB 제한에서는 72프레임·63.3MiB에서 중단하며, Half(320×180)에서는 180프레임·39.6MiB를 유지한다. 레이어 X 위치 341.5217→351.5217 이동·Undo/Redo·저장/재열기, 품질 변경·Clear·외부 소스 mtime 변경 시 캐시 구간 초기화를 확인했다. 캐시 예산 버튼의 Enter/Space 중복 실행을 수정했고 Preview 내용은 스크롤되어 Effects & Presets 헤더와 도크 경계를 유지한다. 캐시 예산과 내용은 프로젝트에 저장하지 않는다.

RAM 프리뷰 검증 프로젝트의 최종 CLI 출력은 640×360 PNG, 30fps·60프레임 MP4(AAC 48kHz 스테레오), 3프레임 MOV(ProRes·PCM 24비트)이며 원래 해상도를 유지했다. 이동으로 생긴 빈 영역은 PNG/MOV 알파 0, MP4는 불투명 검정(손실 압축 샘플 [0,0,2,255])이다. 디스크 캐시·캐시 설정 영속화·장시간 프레임 지연 측정은 남아 있으므로 J02 전체 완료로 표시하지 않는다.


## 최초 조사 시점의 구현 범위

| 영역 | 확인된 구현 | 아직 제한되는 부분 | 코드 근거 |
| --- | --- | --- | --- |
| 프로젝트 | JSON 저장·열기, 원자적 파일 교체, 복구, 버전 1–6 읽기 | 단일 컴포지션, 16 MiB 파일 제한, 공용 복구 슬롯 하나 | `crates/core/src/lib.rs`의 `Project`, `src/project_io.rs`, `src/editor_io.rs` |
| 컴포지션 | 크기·정수 FPS·길이·배경색 | 여러 컴포지션, 소수 프레임 레이트, 중첩 없음 | `Composition`, `ConfigureComposition` |
| 레이어 | 사각형·텍스트·내장 이미지·링크 영상, 배경 Solid | Null·Adjustment·Precomp·Audio·정식 Shape 모델 없음 | `crates/core/src/editing.rs`의 `Content` |
| 편집 | 선택·복제·분할·트림·부모 연결, 이동·회전·스케일·앵커 조작 | 레이어 복사 붙여넣기, Solo/Shy, 다중 변형 일부, 정렬·분배 확장 | `src/editor.rs`, `src/panels/transform_gesture.rs` |
| 애니메이션 | 변형 8개 채널, 키 선택·이동·복사, 선형·Hold·Smooth·시간 Bezier | 다른 속성 애니메이션, 공간 경로, 속도 그래프, AE 방식 양방향 핸들 없음 | `Property`, `AnimatedProperty`, `src/panels/graph.rs` |
| 텍스트 | Wanted Sans, 문자열·크기·단색, 명시적 줄바꿈 | 단락 상자·정렬·폰트 선택·문자별 스타일·텍스트 애니메이터 없음 | `Content::Text`, `src/rendering.rs` |
| 마스크와 효과 | 사각형 마스크 하나와 반전, Blur·Brightness·Grayscale | 경로·다중 마스크·효과 순서·효과 키프레임 없음 | `Mask`, `Effects`, `Renderer::render` |
| 영상 | FFprobe 가져오기, FFmpeg 프레임 읽기, 재연결, 속도·역재생·정지 | 오디오·연속 디코더·프록시·가변 속도 맵·프레임 블렌딩 없음 | `src/footage.rs`, `VideoPlayback` |
| 출력 | 배경 합성 MP4, 알파 MOV, 알파/배경 PNG 및 시퀀스, 명령행 렌더 | 단일 활성 작업, 고정 프리셋, 무음, 색 관리 없음 | `src/video_export.rs`, `src/editor_io.rs`, `src/cli.rs` |
| UI | Project·Composition·Timeline·우측 Properties/Info/Preview·Align, 분할 크기 조절 | 실제 자산 트리, 도킹·분리·워크스페이스 저장, 다양한 전문 패널 없음 | `src/shell.rs`, `src/panels/browser.rs`, `src/panels/sidebar.rs` |

기본 기능이 존재한다는 사실과 AE와 동등하게 구현됐다는 판단은 다르다. 특히 현재 F9는 선택 키의 **나가는 구간**에 적용되는 Ease 프리셋이며 AE Easy Ease 전체 동작과 같지 않다. 배경 Solid도 별도 Solid 자산이 아니라 컴포지션 크기의 사각형 레이어다. 프로젝트 브라우저는 현재 컴포지션 한 항목만 표시한다.

## 먼저 보강할 안정성과 정확성

아래는 구조에서 확인된 제약과 재현이 필요한 잠재 문제를 구분한 것이다. 사용자 원본을 손상시키는 실험은 수행하지 않았다.

| 항목 | 조사 결과와 상태 | 필요한 조치와 검증 |
| --- | --- | --- |
| 복구 슬롯 충돌 | 확인: 모든 앱 인스턴스가 같은 `recovery.lfe.json` 경로 사용. 실제 충돌 실험은 미실시 | 문서·세션별 슬롯, 소유권 관리, 두 인스턴스의 저장·종료·충돌 후 각 문서 복원 |
| 저장 용량 초과 | 확인: 이미지 데이터는 레이어 JSON에 포함되며 저장 한도는 전체 16 MiB. 개별 가져오기 성공이 전체 저장 성공을 보장하지 않음 | 자산 중복 제거와 외부/패키지 저장, 총량 사전 검사, 한도 초과 시에도 작업 회수 경로 제공 |
| 출력이 원본 영상을 덮는 경로 | 잠재 문제: `encode`는 성공 시 목적지를 교체하지만 링크 소스와 목적지의 동일성 검사가 보이지 않음 | 임시 파일로 재현 후 동일 경로·경로 별칭·가능한 파일 동일성 검사. 원본과 같은 목적지는 거부 |
| 허용 크기와 출력 한도 불일치 | 확인: 모델은 각 축 16384까지 허용하지만 렌더러는 33,554,432 픽셀까지 | 설정 단계에서 납품 가능 여부 표시. 출력 불가 설정 차단 또는 타일 렌더 계획. 경계값 검사 |
| 색상 메타데이터 | 확인: 출력 명령에 명시적인 색 원색·전달 함수·매트릭스·범위 정책 없음. 플레이어별 편차 정도는 미측정 | SDR 정책부터 정의하고 Rec.709/sRGB 변환·태그·레벨을 검증. 배경색 구현 완료와 색 관리 완료를 혼동하지 않기 |
| Undo와 미리보기 메모리 | 확인: 편집 명령이 프로젝트 전체를 복제하고 Undo가 최대 100개 스냅샷 보관. 이미지 문자열도 모델에 포함 | 자산 공유 및 증분/구조 공유 스냅샷. 이미지 많은 프로젝트에서 최대 메모리와 입력 지연 측정 |
| 영상 재생 비용 | 확인: 캐시 미스마다 FFmpeg를 새로 실행하고 PNG로 받아 다시 합성 | 지속 디코더와 순차 프리페치. 먼저 프로세스 실행·디코딩·PNG·SVG 파싱별 비용 계측 |
| 자동화 검증 누락 | 확인: 기존 CI는 `moon ci`; core 테스트 태스크는 있으나 desktop 태스크는 dev/check/build만 정의 | desktop 단위 테스트·명시적 FFmpeg 통합 테스트·포맷 검사 태스크를 CI에 연결. 실제 CI 성공 여부는 이번 조사에서 미확인 |

## 우선순위와 완료 판단

- **P0**: 저장·복구·원본 보호·기본 출력 정확성. 실제 작업을 맡기기 전에 해결한다.
- **P1**: 일반적인 2D 모션 작업을 완주하는 핵심 기능. 프로젝트 기반 확장 후 차례로 연결한다.
- **P2**: 편집 속도와 고급 2D 표현을 개선하는 기능. P1과 공유되는 기반을 먼저 구축한다.
- **P3**: 3D·추적·호환성 등 별도 대형 개발 축. 지원 범위를 따로 결정한다.

이하 표의 개발 범위와 완료 기준은 제안이다. 현재 부분 구현은 각 영역 앞에 명시했다. 완료는 버튼이 생긴 시점이 아니라 **편집 → Undo/Redo → 저장·재열기 → 미리보기 → 출력**에서 결과가 일관되는 시점으로 판단한다.

## 프로젝트와 자산

현재 단일 컴포지션과 레이어에 직접 포함된 콘텐츠 구조를 확장해야 한다. AE는 다른 컴포지션을 레이어 소스로 사용하는 중첩 구조를 제공한다. [Adobe 프리컴포지션 문서](https://helpx.adobe.com/after-effects/desktop/work-with-compositions/precomposing-and-nesting/precomposing-nesting-pre-rendering.html)

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| A01 | P1 | `CompositionId`, 컴포지션 목록, 활성 컴포지션, 생성·복제·삭제·탭 | 한 프로젝트의 여러 컴포지션을 독립 편집하고 저장·복원; 참조 중 삭제 처리 |
| A02 | P1 | 자산 ID, 폴더·검색·정렬·정보·썸네일, 자산에서 레이어 생성 | 같은 소스를 여러 레이어가 재사용하고 재연결 결과가 일관됨 |
| A03 | P1 | Pre-compose와 중첩 렌더, 시작 시점·길이·프레임 레이트 변환 | 선택 레이어를 중첩해도 합의한 변환 정책에서 결과 유지; 순환 참조 거부 |
| A04 | P0 | 문서·세션별 자동 저장, 백업 이력, 복구 목록 | 두 인스턴스가 서로의 복구 파일을 지우지 않으며 비정상 종료 후 복원 |
| A05 | P0 | 이미지 중복 제거, 대형 프로젝트 저장 방식, 저장 전 용량 검사 | 현재 JSON 한도를 넘는 미디어 작업도 저장 가능하거나 가져오기 전 명확히 제한 |
| A06 | P1 | 프로젝트 상대 경로, Collect Files, 미디어 누락 목록과 일괄 재연결 | 프로젝트 폴더를 다른 위치로 이동해도 렌더 가능; 미디어 복사 실패 보고 |
| A07 | P1 | 유리수 FPS, 시작 타임코드, 길이 입력 단위·프리셋, 설정 시 출력 한도 검사 | 24000/1001·30000/1001에서 시간 계산과 인코딩 일치; 기존 파일 마이그레이션 |
| A08 | P1 | 이미지 시퀀스·다중 파일 가져오기, 소스 해석, 소스 기반 컴포지션 생성 | 누락 프레임·FPS·알파 처리 명시; 잘못된 가져오기를 하나의 Undo로 취소 |
| A09 | P1 | 워크 영역·컴포지션별 재생 위치·뷰 상태 저장 정책 | 현재 세션 상태인 B/N 범위를 재열기 후 복원; 출력 범위와 편집 편의 설정의 저장 위치 구분 |

## UI와 작업 공간

기존 기본 배치와 Wanted Sans·Gravity Icons를 유지하면서 기능을 제자리에 넣어야 한다. AE는 패널 이동·그룹화·분리·워크스페이스 저장을 제공한다. [Adobe 작업 공간 문서](https://helpx.adobe.com/after-effects/desktop/get-started/get-familiar-with-the-interface/workspaces-panels-viewers.html)

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| B01 | P1 | 기본 레이아웃 기준 이미지와 크기·간격·행 높이 규격 | 같은 화면 크기와 DPI에서 Project/Composition/Timeline/우측 도크 경계 비교; 기능 추가 후 배치 회귀 없음 |
| B02 | P2 | 탭 도킹·패널 표시/숨김·분리 창·워크스페이스 저장/복원 | 재시작과 모니터 변경 뒤 패널을 잃지 않고 Default로 복구 |
| B03 | P1 | Effect Controls와 Effects & Presets 패널 | 선택 레이어의 실제 효과 목록과 검색 결과 연결; 빈 장식 패널 금지 |
| B04 | P2 | Footage/Layer 뷰어, 컴포지션 탭 잠금·탐색 이력 | 원본·레이어 공간·합성 결과를 구분하고 선택 변경에도 고정 뷰 유지 |
| B05 | P1 | 메뉴 키보드 이동·활성 패널 강조·단축키 설정·검색 가능한 명령 | 텍스트 입력/IME/모달과 단축키 충돌 없음; 키보드만으로 주요 작업 가능 |
| B06 | P1 | 색 선택기, RGB/HEX/알파, 스포이트, 최근 색상 | 배경색과 레이어색 필드에 재사용; 색 관리 정책과 일치 |
| B07 | P1 | 화면 읽기 이름·역할, 포커스 순서, DPI·한글 입력 회귀 검증 | 100/125/150/200% 배율에서 입력·팝업 잘림 검사; IME 조합 중 단축키 오작동 없음 |
| B08 | P2 | 최근 프로젝트, 설정, 알림·문제 목록, 작업 로그 | 저장/미디어/렌더 오류가 다음 상태 메시지에 묻히지 않고 다시 확인 가능 |
| B09 | P1 | 눈금자·가이드·그리드·스냅·Title/Action Safe·RGB/알파 채널 보기·픽셀 정보 | 안내선은 출력 제외; 줌/팬과 좌표 일치; 실제 색/알파와 Info 표시 비교 |

## 레이어와 타임라인

다중 선택·분할·트림·키 이동·부모 연결은 이미 있다. 아래는 기존 편집의 확장이다.

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| C01 | P1 | Null, 독립 Solid 설정, Adjustment 레이어 | Null은 출력에 나타나지 않고 부모 역할 수행; Solid 크기·색 독립 편집; Adjustment는 정의된 아래 합성 결과에 적용 |
| C02 | P1 | Solo·Shy·레이어 라벨·Guide·스위치/모드 열 | Solo와 가시성 관계 명시; Guide가 최종 출력에서 제외; 열 전환 시 시간축 정렬 유지 |
| C03 | P1 | 레이어 Copy/Cut/Paste와 다른 컴포지션에 붙여넣기 | 자산·부모·키·마스크 참조를 안전하게 새 ID로 매핑; 잠긴 레이어 원자성 유지 |
| C04 | P1 | 컴포지션·레이어 마커, 이름·색·기간, 마커 탐색 | 이동/분할/중첩 시 시간 기준 일관; 저장 후 복원 |
| C05 | P1 | 시간축 스냅, 플레이헤드·키·레이어 경계·마커 정렬 | 줌 배율에 따른 스냅 허용거리와 해제 키 동작 일관 |
| C06 | P2 | 시퀀스 레이어, 시간 스트레치, 키 시간 역전·비례 조정, Slip | 소스 시간·레이어 범위·키 시간을 구분; 충돌·음수 시간 처리와 Undo 검사 |
| C07 | P1 | 다중 레이어 회전·스케일, 선택 기준 정렬·분배, Pick Whip 부모 연결 | 부모/자식 동시 선택의 이중 변형 방지; 회전·음수 스케일 포함 검증 |
| C08 | P2 | 계층별 펼치기·수정 속성 표시·복합 검색·타임라인 행 가상화 | 수백 레이어에서 표시/선택/드래그 좌표 일치; 측정된 입력 지연 목표 충족 |
| C09 | P2 | Transform Reset·Fit to Comp·반전·앵커 중앙 이동·Auto Orient·Skew | 정적/애니메이션/부모 연결별 적용 정책과 Undo 검증; 현재 포즈 보존과 전체 애니메이션 베이크를 구분 |

## 속성과 애니메이션

현재 키프레임 대상은 변형 8개 채널뿐이다. 먼저 색·벡터·문자열·경로·효과 파라미터를 다룰 공용 속성 모델이 필요하다. AE는 시간 보간과 공간 보간을 구분하므로, 현재 값 그래프를 확장하는 것만으로 위치 경로가 완성되지는 않는다. [Adobe 보간 문서](https://helpx.adobe.com/after-effects/desktop/animate-in-after-effects/animation-keyframes/keyframe-interpolation.html)

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| D01 | P1 | 타입이 있는 속성 경로·애니메이션 트랙·기본값·제약·수정 상태 | 효과·색·마스크도 같은 키 편집/저장/Undo 체계 사용; 스키마 마이그레이션 |
| D02 | P1 | 독립 incoming/outgoing 시간 핸들, 속도·영향도, Auto/Continuous Bezier와 Easy Ease | 양쪽 구간 편집이 예상대로 반영; 기존 Bezier 파일의 재생 결과 보존 |
| D03 | P1 | 다중 채널 값 그래프, Speed Graph, 키 일괄 선택·정렬·스케일 | 단위가 다른 채널의 표시 규칙 명시; 속도와 시간·값 그래프 관계 검증 |
| D04 | P2 | 위치의 공간 Bezier, 모션 경로 점·탄젠트, 로빙 키 | 시간 보간과 경로 모양을 독립 편집하고 미리보기·출력이 동일 경로 사용 |
| D05 | P1 | 애니메이션 가능한 Time Remap | 소스 시간 트랙으로 가속·감속·정지·역재생; 소스 밖·중첩·오디오 정책 명시 |
| D06 | P2 | 애니메이션 프리셋·변형 복사·속성 선택 붙여넣기 | 여러 속성의 상대 시간과 보간을 다른 레이어에 재사용; 없는 속성 처리 |
| D07 | P2 | 표현식 기초와 컨트롤러, 시간·참조·루프·난수 | 순환 참조·오류·평가 시간 제한, 미리보기와 출력의 결정적 결과; AE 언어 호환은 별도 명세 |

## 도형과 경로

현재 사각형은 단일 채움 도형이다. 일반 Shape 레이어와 Pen 편집은 신규 구현이다. [Adobe 도형과 Pen 도구 문서](https://helpx.adobe.com/after-effects/desktop/drawing-painting-and-paths/shapes-and-shape-attributes/creating-shapes-masks.html)

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| E01 | P1 | 타원·둥근 사각형·다각형·별·선, 드래그 생성 | 선택 상태에 따른 생성 대상 명확; 크기·비율·원점 편집과 Undo |
| E02 | P1 | Pen, 열린/닫힌 Bezier, 점 추가·삭제·변환 | 줌/팬/부모 변형 아래에서 포인터와 경로 점 일치 |
| E03 | P1 | Fill/Stroke·두께·캡·조인·Dash·선형/방사형 그라디언트 | 경계·투명도·그라디언트 변형이 저장 후 동일 |
| E04 | P2 | Shape 그룹·그룹 변형·여러 Fill/Stroke와 연산 순서 | 그룹 중첩과 순서를 바꾸면 합의된 렌더 순서대로 결과 변화 |
| E05 | P2 | Trim Paths·Repeater·Merge/Offset Paths와 경로 애니메이션 | 연산 순서/닫힌 경로/다른 점 수 보간 정책 정의 및 출력 검증 |
| E06 | P2 | SVG 가져오기와 가능한 요소의 편집형 변환 | 지원 범위 보고; 지원하지 않는 요소를 조용히 누락하지 않음 |

## 텍스트와 타이포그래피

Wanted Sans는 앱 UI 기본 폰트로 유지한다. 콘텐츠 폰트 선택은 UI 폰트 변경과 별개의 기능이다.

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| F01 | P1 | 텍스트 도구, 캔버스 직접 편집, Point/Paragraph Text | 한글 IME·여러 줄·선택·붙여넣기·취소·Undo 동작 일관 |
| F02 | P1 | 글꼴·굵기·스타일·대체 글꼴·누락 글꼴 관리 | 미리보기와 출력 동일 글꼴; 다른 PC에서 누락 상태와 대체 결과 명시 |
| F03 | P1 | 정렬·행간·자간·커닝·자동 줄바꿈·문자별 스타일·Stroke | 박스 크기와 문자 경계가 실제 렌더와 일치; 저장 후 레이아웃 유지 |
| F04 | P2 | Text Animator·Range Selector·문자/단어/줄 단위 변형 | 문자열 수정 후에도 범위 의미 유지; 한글 조합 문자 단위 분리 검사 |
| F05 | P2 | 경로 위 텍스트·Source Text 애니메이션·재사용 텍스트 컨트롤 | 경로/텍스트 변경·중첩 인스턴스별 값·내보내기 결과 일치 |

## 마스크와 합성 및 효과

`Effects` 고정 구조를 순서 있는 효과 목록으로 바꾸고 렌더 순서를 명문화해야 한다. AE는 효과·애니메이션 프리셋을 재사용하며, Track Matte는 다른 레이어의 알파 또는 밝기를 참조한다. [Adobe 효과와 프리셋](https://helpx.adobe.com/after-effects/desktop/apply-effects-and-animation-presets/effects-and-animation-presets/effects-animation-presets-overview.html), [Adobe Track Matte](https://helpx.adobe.com/after-effects/desktop/work-with-transparency-and-compositing/work-with-track-mattes-and-traveling-mattes/track-mattes-and-traveling-mattes.html)

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| G01 | P1 | 다중 경로 마스크·Add/Subtract/Intersect·반전·Feather·Expansion·Opacity | 마스크 순서와 경계 동작 정의; 경로·수치 애니메이션 및 투명 경계 검사 |
| G02 | P1 | Alpha/Luma와 반전 Track Matte·소스 선택·참조 재사용 | 레이어 순서와 독립된 참조, 순환 참조 거부, 이동·삭제·복제 시 일관성 |
| G03 | P1 | Blend Modes와 조정 레이어 합성 규칙 | 우선 Normal/Multiply/Screen/Add/Overlay의 알파 포함 기준 이미지 비교 |
| G04 | P1 | 효과 인스턴스 목록·추가·삭제·중복·순서·우회·Reset·키프레임 | 같은 효과 여러 개, 순서 변경, 저장/Undo; 기존 세 효과 결과 마이그레이션 |
| G05 | P1 | 기본 실무 효과군: Fill/Tint·Levels/Curves·Hue/Saturation·Glow·Drop Shadow·Gradient | 각 효과의 파라미터 범위·좌표 공간·경계·투명도·출력 테스트 |
| G06 | P2 | 키잉·매트 정리·변위·왜곡·노이즈·전환·레이어 스타일 | 효과군을 개별 작업으로 분해; 참조 레이어와 애니메이션 평가 순서 검증 |
| G07 | P2 | 모션 블러·셔터·서브프레임 평가, 프레임 블렌딩 | 빠른 이동·회전·중첩·속도 변경에서 시간 샘플링 비교; 성능 예산 마련 |

## 오디오

최초 조사 당시 가져오기·미리보기·출력 모두 오디오를 다루지 않았다. 이후 H01 소스 메타데이터·파형과 H04 공통 믹서·AAC/PCM 영상 출력을 구현했다. H03 좌우 레벨·Pan·Fade 편집과 구간 미터를 추가했다. H02에 Windows 기본 장치 재생·스크럽·장치 시계 플레이헤드·재생 블록 미터를 추가했다. 영상 디코더/표시 지연 개선과 확장 장치 관리는 남아 있다. 출력 믹서는 선형 보간 기반으로 고속 리샘플링 품질 개선이 남아 있다.

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| H01 | P1 | 오디오 자산·영상 내 오디오·채널·샘플레이트·파형 | 독립 파일과 영상 소스를 읽고 트림·이동·분할 후 파형 시간 일치 |
| H02 | P1 | 오디오 장치·재생/스크럽·A/V 동기화·캐시 재생 | 1분 이상 기준 클립에서 시간 드리프트 측정; Seek·Loop·Pause 후 동기 유지 |
| H03 | P1 | 레벨·Mute/Solo·페이드·Pan·키프레임·기본 미터 | 클리핑·무음 구간·복수 트랙 혼합을 정의된 샘플 기준으로 검증 |
| H04 | P1 | AAC/PCM 포함 출력, 워크 영역·중첩·속도 변경에 따른 오디오 처리 | 영상과 시작/끝 일치; 비정수 FPS의 샘플 수, 역재생·정지 정책 명시 |

## 렌더링과 색 관리

배경색과 알파 출력은 이미 구현했다. 아래는 납품 옵션과 일관성 확장이다. AE의 Render Queue는 여러 작업과 출력 모듈을 관리하고, 색 관리는 입력·작업·출력 공간을 연결한다. [Adobe 렌더 큐](https://helpx.adobe.com/after-effects/desktop/render-and-export/basics-of-rendering-and-exporting/basics-rendering-exporting.html), [Adobe 색 관리](https://helpx.adobe.com/after-effects/desktop/adjust-colors/color-management/color-management.html)

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| I01 | P0 | 출력 전 점검: 원본 경로 보호·해상도·누락 미디어·글꼴·인코더·쓰기 권한 | 실패 시 기존 목적지와 원본 유지; 오류 원인 및 수정 경로 표시 |
| I02 | P1 | Render Queue·작업 스냅샷·순서·재시도·프리셋·여러 출력 모듈 | 여러 컴포지션/포맷 무인 렌더, 재시작 후 작업 상태 복원; 하나의 실패가 나머지 정책대로 처리 |
| I03 | P1 | 해상도·FPS·범위·CRF/비트레이트·코덱·오디오·채널 옵션 | 선택값이 실제 스트림 메타데이터와 일치; 불가능한 조합 사전 거부 |
| I04 | P0 | SDR 색 공간·전달 함수·YUV 매트릭스/범위·태그·알파 해석 정책 | 색 패치와 반투명 경계를 여러 디코더로 비교; MP4 허용 손실과 PNG 정확성 구분 |
| I05 | P2 | 16/32비트·선형 합성·ICC/OCIO·HDR·EXR 등 고급 입출력 | 8비트 한계를 실제 렌더 버퍼부터 제거; HDR 메타데이터·톤 매핑·밴딩 검증 |
| I06 | P2 | CLI 작업 명세·기계 판독 진행률·취소·오류 코드·시퀀스 재개 | 자동화에서 성공/실패/취소 구분; 재개 시 프로젝트/설정 일치 검사 |

## 미리보기와 성능

현재 CPU 기반 SVG/resvg 합성과 제한된 영상 PNG 캐시를 사용한다. 병목 측정 없이 전체 GPU 재작성부터 시작하지 않는다.

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| J01 | P1 | 지속 영상 디코더·순차 프리페치·빠른 탐색·오래된 요청 취소 | 순차 재생의 프로세스 재시작 제거; Seek 후 예전 프레임이 덮이지 않음 |
| J02 | P1 | RAM 프리뷰·캐시 구간 표시·디스크 캐시·정리·예산 설정 | 캐시된 워크 영역의 안정 재생; 파일/효과/부모 변경 시 관련 캐시 무효화 |
| J03 | P1 | 공유 자산·증분 평가·Undo 메모리 예산·렌더 성능 계측 | 이미지/영상/효과별 기준 프로젝트의 메모리·Seek 지연·프레임 시간 기록 |
| J04 | P2 | Proxy·ROI·적응형 품질·멀티프레임 렌더·GPU 가속 | 측정 병목부터 개선; CPU 기준 출력과 비교하고 미리보기 품질이 최종 출력에 새지 않음 |

## 고급 제작과 호환성

이 영역은 기본 2D 작업 완성과 별도의 큰 개발 단계다. Adobe 고유 포맷/엔진/플러그인과 동일하게 동작한다고 전제하지 않는다. 분야 구분은 [Adobe 공식 사용 안내](https://helpx.adobe.com/after-effects/desktop.html)의 3D·추적·로토·표현식·자동화 범주를 참고했다.

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| K01 | P3 | 3D 변형·카메라·라이트·뷰·그림자·심도 | 좌표/합성 규칙과 렌더러 설계 후 기준 장면 비교; 2D와 연결 |
| K02 | P3 | 3D 모델·재질·환경광·텍스트/도형 돌출 | 지원 포맷과 셰이딩 범위 명시; 외부 도구 의존성 및 이식성 검증 |
| K03 | P3 | 2D/평면/카메라 추적·안정화·마스크 추적 | 추적 결과 편집/재사용, 실패 구간 표시, 실제 촬영 기준 데이터 검증 |
| K04 | P3 | 로토·페인트·Puppet·콘텐츠 제거·시뮬레이션 | 각각 별도 명세와 성능/품질 데이터로 평가; 장식 UI로 완료 처리하지 않기 |
| K05 | P2 | 재사용 템플릿·노출 컨트롤·CSV/JSON 기반 텍스트/수치 교체 | 중첩 인스턴스별 오버라이드, 데이터 누락 처리, 배치 렌더 |
| K06 | P3 | 스크립트 API·플러그인·AEP/PSD/AI/MOGRT 등 교환 범위 연구 | 포맷별 가져올 정보/손실/미지원 항목 보고; 호환 테스트 파일 확보 후 지원 선언 |

## 제품화와 검증 체계

| ID | 우선순위 | 개발 범위 | 완료 기준 |
| --- | --- | --- | --- |
| L01 | P0 | desktop 테스트·FFmpeg 통합·fmt·스키마/렌더 회귀를 CI 태스크로 연결 | 실제 파이프라인에서 실행 사실과 결과 확인; ignored 테스트를 실행하지 않고 통과로 간주하지 않음 |
| L02 | P1 | 안정적인 설치/업데이트·배포 경로·FFmpeg 탐지와 설정·진단 | 깨끗한 Windows 사용자 환경에서 열기→편집→저장→출력; 기존 프로젝트 보존 |
| L03 | P1 | 재현 프로젝트·성능 기준·UI 조작 회귀·배율별 기준 화면 | 단일 프레임 비교에 더해 재생/드래그/모달/저장 실패/취소의 상태 전이 검사 |
| L04 | P2 | 한국어 UI·도움말·샘플·단축키 문서·오류 안내·릴리스 노트 | 기능/메뉴/문서 일치; 기존 포맷과 마이그레이션의 사용자 안내 |
| L05 | P3 | 웹/API 제품 범위와 데스크톱 모델 공유 여부 결정 | 현재 웹 `/editor`는 Coming soon, API는 health/echo 수준임을 기준으로 별도 일정 수립 |

## 권장 구현 순서와 선행 조건

1. **저장과 출력의 신뢰성**: A04/A05, I01/I04, L01. B01의 레이아웃 기준도 이때 고정한다. 원본 경로 문제는 복사된 테스트 파일로 재현한 뒤 수정한다.
2. **확장 가능한 프로젝트 기반**: A01/A02/A07과 D01. 컴포지션·자산 ID와 타입 속성, 시간 표현, 저장 마이그레이션을 정한다. 기존 단일 컴포지션 예제의 출력이 유지돼야 한다.
3. **실무 2D 작업 연결**: A03/A06/A08/A09, C01–C05/C07, G02–G05, B03/B09, I02/I03. 프리컴포지션·자산 재사용·매트·효과가 하나의 프로젝트에서 함께 동작하도록 만든다.
4. **소리와 편집 응답성**: H01–H04, J01–J03. 시간 모델을 공통으로 사용하고 장시간 동기화·메모리를 검증한다.
5. **표현과 편집 정교화**: E01–E03, G01, F01–F03, D02/D03/D05, B05–B07. 경로·텍스트·키프레임을 실제 UI에서 편집하고 출력까지 연결한다.
6. **고급 2D 제작과 제품화**: 남은 P2와 L02–L04. 자주 쓰는 작업을 기준으로 프리셋·표현식·모션 블러·캐시·템플릿을 추가한다.
7. **3D와 호환성**: K01–K04/K06, L05. 각 기능군의 별도 설계와 완료 기준을 확정한 뒤 진행한다.

주요 의존 관계는 다음과 같다.

| 선행 기반 | 그 기반을 사용하는 기능 |
| --- | --- |
| A01/A02 컴포지션·자산 ID | A03 프리컴포지션, A06 프로젝트 이동, C03 복사, I02 렌더 큐, K05 템플릿 |
| A07 공통 시간 모델 | H02/H04 오디오 동기화, D05 Time Remap, G07 모션 블러, I03 FPS 출력 |
| D01 타입 속성과 트랙 | G04 효과 애니메이션, G01 마스크 애니메이션, F04/F05 텍스트, D07 표현식 |
| E02 경로 모델 | G01 경로 마스크, E05 경로 연산, F05 경로 텍스트 |
| G04 순서 있는 효과 평가 | C01 Adjustment, G05/G06 효과군, J04 가속 |
| I04 색과 알파 정책 | G03 블렌딩, I05 고정밀/HDR, J04 GPU 출력 비교 |
| J01/J02 디코딩과 캐시 | H02 실시간 동기화, J04 Proxy/가속, 복잡한 중첩 미리보기 |

이 순서는 작업 의존성을 표현하며 인력이나 기간 견적이 아니다. 특히 3D·로토·AE 호환성은 개별 기능 하나로 끝나는 작업이 아니다.

## 실사용 가능 단계의 검증 시나리오

아래는 향후 릴리스의 합격 기준 제안이며 현재 모두 통과했다는 뜻이 아니다.

- 1080p 컴포지션 여러 개, 재사용 타이틀 프리컴포지션, 링크 영상·텍스트·도형·음악으로 60초 영상을 제작한다.
- Track Matte와 두 개 이상의 순서 있는 효과를 적용하고 효과·마스크·오디오 레벨을 애니메이션한다.
- 비정수 FPS 장면에서 워크 영역 MP4의 영상/소리 시작과 끝을 확인하고 MOV/PNG의 알파가 보존되는지 검사한다.
- 저장 후 재시작, 폴더 이동, 누락 미디어 재연결, 복구, Undo/Redo를 거쳐 기준 프레임과 시간 정보를 비교한다.
- 렌더 취소·인코더 실패·디스크 쓰기 실패·원본과 같은 출력 경로를 검사하고 원본/기존 출력 보존을 확인한다.
- 타임라인/그래프/컴포지션을 오가며 편집하고, 100–200% DPI 및 한글 입력에서 패널 배치와 포커스가 유지되는지 확인한다.
- 성능은 기준 PC와 프로젝트를 기록한 뒤 캐시된 재생 FPS, Seek 지연, 입력 지연, 메모리 상한으로 평가한다. 측정 전 실시간 성능을 보장하지 않는다.

## 조사 근거와 후속 확인

코드 기준은 `crates/core/src/lib.rs`, `crates/core/src/editing.rs`와 데스크톱의 `src/editor.rs`, `src/editor_io.rs`, `src/project_io.rs`, `src/footage.rs`, `src/rendering.rs`, `src/video_export.rs`, `src/cli.rs`, `src/shell.rs`, `src/panels/`다. CI 범위는 `.github/workflows/`, `apps/desktop/moon.yml`, `crates/core/moon.yml`을 확인했다. 웹/API는 편집기 진입 화면·매니페스트·API 진입점만 확인했으며 전체 웹 기능 감사는 하지 않았다.

직전 구현 보고의 기본 테스트 70개 및 FFmpeg 통합 테스트 6개 통과는 배경색 기능까지의 검증 이력이다. 이 문서에 나열한 미구현 기능을 검증한 숫자가 아니며, 이번 문서 작업에서는 테스트를 다시 실행하지 않았다.

다음 실제 개발 착수 지점은 **A04 복구 슬롯 분리, A05 저장 용량/자산 중복 처리, I01 출력 원본 보호와 사전 점검**이다. 이후 A01/A02/D01을 함께 설계해야 프리컴포지션·효과·오디오를 반복적인 데이터 구조 재작성 없이 확장할 수 있다.
