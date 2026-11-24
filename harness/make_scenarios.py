"""시나리오 파일(scenarios.json)을 만든다. 긴 글과 시험용 자료를 읽기 쉽게 적으려고 Python으로 둔다.

    python harness/make_scenarios.py

시험용 자료는 모두 지어낸 것이며 실제 회사·사람의 정보가 아니다.
"""
import json
import pathlib

SALES_CSV = """월,제품A,제품B,제품C
2025-01,1200,800,450
2025-02,1150,820,470
2025-03,1300,790,520
2025-04,1280,850,560
2025-05,1350,870,610
2025-06,1400,900,640
2025-07,1380,860,700
2025-08,1320,840,720
2025-09,1450,910,760
2025-10,1500,930,800
2025-11,1550,950,850
2025-12,1700,1000,900
"""
# 분기 합계: 1분기 7,500 / 2분기 8,460 / 3분기 8,940 / 4분기 10,180, 연간 35,080 (단위: 만원)

MEETING = """# 2025년 12월 영업 회의록

- 단위는 만원이다.
- 제품C는 연초 대비 두 배로 성장했다. 온라인 채널 확대가 주된 이유다.
- 제품B는 성장이 더디다. 경쟁사 가격 인하의 영향으로 본다.
- 8월은 휴가철이라 제품A 매출이 잠시 줄었다.
- 2026년 목표: 전체 매출 15% 성장.
"""

README = """# 재고 관리 도구

현재 버전: 1.2.0

## 설치

`setup.exe`를 실행합니다.

## 변경 이력

- 1.2.0 - 바코드 검색 추가
- 1.1.0 - 재고 알림 추가
- 1.0.0 - 첫 배포
"""

WEEK_NOTES = """# 11월 2주차 메모 (11/9~11/13)

- 월: 신규 고객사 2곳 미팅, 견적서 발송
- 화: 재고 관리 도구 1.3.0 시험, 오류 3건 발견
- 수: 오류 3건 중 2건 수정
- 목: 분기 매출 자료 정리
- 금: 남은 오류 1건은 다음 주 화요일까지 수정 예정
"""

WEEKLY_SKILL = """---
name: weekly-report
description: 주간 업무 보고서를 회사 양식으로 작성한다. 사용자가 주간 보고, 주간 업무 보고서, 이번 주 업무 정리를 요청할 때 쓴다.
---

# 주간 업무 보고서 양식

보고서는 반드시 아래 순서와 제목을 그대로 씁니다.

1. `## 1. 이번 주 한 일` - 표로 작성 (열: 요일, 내용, 결과)
2. `## 2. 다음 주 할 일` - 번호 목록
3. `## 3. 요청·이슈` - 없으면 "없음"

맨 마지막 줄에는 반드시 `보고 양식 버전: W-7` 이라고 적습니다.
파일은 `reports/주간보고_YYYY-MM-DD.md` (보고서를 쓰는 날짜)로 저장합니다.
"""

scenarios = [
    {
        "id": "chat-date",
        "note": "시스템 프롬프트의 현재 시각을 믿고 날짜 계산을 하는지",
        "mode": "chat",
        "turns": ["오늘 날짜와 요일을 알려 주고, 올해 크리스마스까지 며칠 남았는지 계산해 주세요."],
        "checks": [
            {"kind": "answerContains", "text": "11월 15일"},
            {"kind": "answerContains", "text": "일요일"},
            {"kind": "answerContains", "text": "40"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "chat-long-report",
        "note": "긴 답변: 출력이 중간에 끊기는지(finish_reason), 속도, 추론 분량",
        "mode": "chat",
        "turns": [
            "직원 50명 규모의 중소 제조기업이 생성형 AI를 업무에 도입하려고 합니다. 경영진에게 보고할 도입 전략 보고서를 써 주세요. "
            "목차는 1) 도입 배경 2) 적용할 업무 3) 단계별 추진 일정 4) 필요 예산 5) 위험과 대응 6) 기대 효과 7) 결론 순서로 하고, "
            "각 장을 700자 이상으로 자세히 써 주세요. 예산과 일정은 표로 정리해 주세요."
        ],
        "checks": [
            {"kind": "answerMinChars", "n": 4500},
            {"kind": "answerContains", "text": "결론"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-report",
        "note": "파일 여러 개를 읽고 계산해서 보고서 저장: 라운드 수, 한 번에 여러 도구를 부르는지, 계산 정확도 (기본 설정)",
        "mode": "project",
        "files": {"data/sales_2025.csv": SALES_CSV, "notes/회의록_2025-12.md": MEETING},
        "turns": [
            "data 폴더의 매출 자료와 notes 폴더의 회의록을 읽고, 분기별 매출 합계 표와 개선 제안 3가지를 담은 보고서를 "
            "reports/2025_매출분석.md 파일로 저장해 줘."
        ],
        "checks": [
            {"kind": "fileExists", "path": "reports/2025_매출분석.md"},
            {"kind": "fileContainsAny", "path": "reports/2025_매출분석.md", "text": "10,180|10180"},
            {"kind": "fileContainsAny", "path": "reports/2025_매출분석.md", "text": "7,500|7500"},
            {"kind": "fileContainsAny", "path": "reports/2025_매출분석.md", "text": "35,080|35080"},
            {"kind": "noToolErrors"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-excel-chart",
        "note": "Python 스크립트 작성 후 실행까지 한 답변에서 끝내는지 (이전 추론 유지 설정)",
        "mode": "project",
        "files": {"data/sales_2025.csv": SALES_CSV},
        "approve": ["run_python"],
        "turns": ["data/sales_2025.csv로 제품별 월 매출 추이 꺾은선 차트가 들어간 엑셀 파일 reports/매출차트.xlsx를 만들어 줘."],
        "checks": [
            {"kind": "toolCalled", "text": "run_python"},
            {"kind": "fileExists", "path": "reports/매출차트.xlsx"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-modify",
        "note": "기존 파일 부분 수정: 읽지 않고 고치려다 실패하는지, 낮은 추론 강도에서의 정확도",
        "mode": "project",
        "files": {"README.md": README},
        "turns": ["README.md에서 현재 버전을 1.3.0으로 올리고, 변경 이력 맨 위에 '1.3.0 - 엑셀 내보내기 추가' 항목을 넣어 줘."],
        "checks": [
            {"kind": "fileContains", "path": "README.md", "text": "현재 버전: 1.3.0"},
            {"kind": "fileContains", "path": "README.md", "text": "- 1.3.0 - 엑셀 내보내기 추가\n- 1.2.0 - 바코드 검색 추가"},
            {"kind": "fileContains", "path": "README.md", "text": "- 1.0.0 - 첫 배포"},
            {"kind": "noToolErrors"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-skill",
        "note": "앱이 Skill을 미리 넣고 모델이 그 양식을 따르는지 (이전 추론 유지 설정). 파일 이름 날짜는 실행한 날({today}), 양식 검사는 날짜와 따로(*)",
        "mode": "project",
        "files": {"notes/week.md": WEEK_NOTES},
        "skills": {"weekly-report": WEEKLY_SKILL},
        "turns": ["notes/week.md 메모를 바탕으로 이번 주 주간 업무 보고서를 만들어 줘."],
        "checks": [
            {"kind": "skillAutoLoaded", "text": "weekly-report"},
            {"kind": "fileExists", "path": "reports/주간보고_{today}.md"},
            {"kind": "fileContains", "path": "reports/주간보고_*.md", "text": "보고 양식 버전: W-7"},
            {"kind": "fileContains", "path": "reports/주간보고_*.md", "text": "## 3. 요청·이슈"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "chat-multiturn",
        "note": "여러 번 주고받기: 앞에서 정한 규칙을 계속 지키는지, 앞 답을 기억하는지",
        "mode": "chat",
        "turns": [
            "이 대화에서는 모든 답변을 세 문장 이내로 해 주세요. 그리고 제 이름은 '한결'입니다. 알겠으면 '네'라고만 답하세요.",
            "재택근무의 장점과 단점을 알려 주세요.",
            "방금 답을 표로 바꾸고, 표 아래에 제 이름을 넣어 한 줄 인사를 붙여 주세요.",
        ],
        "checks": [
            {"kind": "answerContains", "text": "한결"},
            {"kind": "answerContains", "text": "|"},
            {"kind": "finished"},
        ],
    },
]

# ---------------------------------------------------------------- 2차: 고친 뒤 확인
SETTINGS_MD = """# 서버 점검 설정

- 담당자: 김하늘
- 점검 주기: 주 1회
- 알림: 이메일
- 보관 기간: 90일
"""

VISITORS_CSV = """요일,방문자
월,312
화,287
수,301
목,334
금,402
토,518
일,476
"""

AI_NOTES = """# 사내 AI 활용 현황 메모

- 직원 40명, 사무직 위주
- 현재 쓰는 곳: 회의록 요약, 이메일 초안, 엑셀 수식 작성
- 걱정: 고객 정보 입력, 결과물 검증 없이 그대로 쓰는 경우
- 요청: 신입도 따라 할 수 있는 쉬운 안내서
"""

scenarios_2 = [
    {
        "id": "proj-long-doc",
        "note": "긴 문서를 파일로 저장: 본문이 한꺼번에 도착할 때까지의 조용한 시간(120초 넘는지)과 끊김 여부",
        "mode": "project",
        "files": {"notes/ai_현황.md": AI_NOTES},
        "turns": [
            "notes/ai_현황.md를 참고해서 사내 AI 활용 안내서를 써 줘. 1) 왜 쓰는가 2) 쓸 수 있는 업무 3) 입력하면 안 되는 정보 "
            "4) 결과 검증 방법 5) 좋은 질문 쓰는 법 6) 자주 묻는 질문 순서로, 각 장을 600자 이상 자세히 쓰고 "
            "docs/AI_활용_안내서.md 파일로 저장해 줘."
        ],
        "checks": [
            {"kind": "fileExists", "path": "docs/AI_활용_안내서.md"},
            {"kind": "fileMinChars", "path": "docs/AI_활용_안내서.md", "n": 3600},
            {"kind": "fileContains", "path": "docs/AI_활용_안내서.md", "text": "자주 묻는 질문"},
            {"kind": "maxCalls", "n": 3},
            {"kind": "noToolErrors"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-modify-2",
        "note": "여러 곳 수정: 한 응답에 수정을 모으고, 다시 읽어 확인하지 않는지 (이전 6회 → 목표 3회)",
        "mode": "project",
        "files": {"config/설정.md": SETTINGS_MD},
        "turns": ["config/설정.md에서 담당자를 이서준으로, 점검 주기를 주 2회로 바꾸고, 목록 맨 끝에 '- 백업: 매일 02:00' 항목을 추가해 줘."],
        "checks": [
            {"kind": "fileContains", "path": "config/설정.md", "text": "- 담당자: 이서준"},
            {"kind": "fileContains", "path": "config/설정.md", "text": "- 점검 주기: 주 2회"},
            {"kind": "fileContains", "path": "config/설정.md", "text": "- 보관 기간: 90일\n- 백업: 매일 02:00"},
            {"kind": "maxCalls", "n": 3},
            {"kind": "noToolErrors"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-excel-2",
        "note": "스크립트 작성과 실행을 한 응답에 모으는지, 실행 뒤 다시 확인하지 않는지 (이전 6회 → 목표 3~4회)",
        "mode": "project",
        "files": {"data/visitors.csv": VISITORS_CSV},
        "approve": ["run_python"],
        "turns": ["data/visitors.csv로 요일별 방문자 막대 차트가 들어간 엑셀 파일 reports/방문자.xlsx를 만들어 줘."],
        "checks": [
            {"kind": "toolCalled", "text": "run_python"},
            {"kind": "fileExists", "path": "reports/방문자.xlsx"},
            {"kind": "maxCalls", "n": 4},
            {"kind": "noToolErrors"},
            {"kind": "finished"},
        ],
    },
]

# ---------------------------------------------------------------- 3차: 문서 형식, 여러 턴, 코드, 긴 입력
COSTS_CSV = """항목,금액
인건비,4200
임차료,1100
재료비,2600
마케팅,900
기타,400
"""

PLAN_NOTES = """# 신사업 메모: 사내 식당 예약 앱

- 문제: 점심시간 대기 줄이 길다 (평균 12분)
- 아이디어: 시간대별 예약과 메뉴 미리 보기
- 대상: 본사 직원 300명
- 비용 추정: 개발 3,000만원, 운영 월 50만원
- 기대: 대기 시간 12분 → 4분
- 일정: 12월 기획, 1~2월 개발, 3월 시범 운영
"""

QUARTER_CSV = """분기,국내,해외
1분기,520,180
2분기,560,210
3분기,610,260
4분기,700,330
"""

MINUTES = """# 11월 12일 제품 회의

참석: 영업, 개발, 디자인 (모두 가상의 팀)

1. 1.3.0 배포는 11월 27일로 확정
2. 엑셀 내보내기에서 날짜 형식이 깨지는 오류가 있음. 개발팀이 11월 18일까지 수정
3. 디자인팀은 새 아이콘 시안 2개를 11월 19일까지 공유
4. 고객사 두 곳이 바코드 일괄 등록 기능을 요청. 다음 버전(1.4.0) 후보로 검토
"""

CALC_PY = """def average(values):
    # 평균을 구한다. 빈 목록이면 0.
    if not values:
        return 0
    return sum(values) / (len(values) - 1)


def discount(price, percent):
    # percent(%)만큼 할인한 가격
    return price - price * percent / 100
"""

TEST_CALC_PY = """import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from calc import average, discount

failed = 0


def check(name, got, want):
    global failed
    if got != want:
        failed += 1
        print(f"FAIL {name}: got {got}, want {want}")
    else:
        print(f"ok   {name}")


check("average of 2,4,6", average([2, 4, 6]), 4)
check("average of empty", average([]), 0)
check("discount 10%", discount(1000, 10), 900)
print("failed:", failed)
sys.exit(1 if failed else 0)
"""

POLICY_FILES = {
    f"docs/정책_{i:02d}_{name}.md": f"# {name}\n\n{body}\n"
    for i, (name, body) in enumerate(
        [
            ("배송", "주문 후 2일 안에 발송합니다. 도서 산간은 하루가 더 걸립니다."),
            ("교환", "받은 날부터 7일 안에 교환을 신청할 수 있습니다."),
            ("환불", "환불 정책: 결제 취소는 3영업일 안에 처리합니다. 포장을 뜯은 상품은 환불 금액의 10%를 뺍니다."),
            ("적립금", "구매 금액의 1%를 적립합니다. 적립금은 1년 뒤 사라집니다."),
            ("회원 등급", "최근 6개월 구매 금액으로 등급을 정합니다."),
            ("고객 응대", "문의는 24시간 안에 답합니다. 환불 정책 문의는 '환불' 문서를 안내합니다."),
            ("개인정보", "주문 정보는 5년 보관합니다."),
            ("제휴", "제휴사 쿠폰은 중복 사용할 수 없습니다."),
        ],
        start=1,
    )
}


def order_log():
    """긴 입력 시험용 주문 기록 (지어낸 자료). 한가운데에 찾을 줄이 있다."""
    import random

    rnd = random.Random(7)
    clients = ["가람상사", "나래물산", "다온전자", "라온식품", "마루건설", "바다해운"]
    staff = ["김", "이", "박", "최", "정", "강"]
    lines = []
    for i in range(520):
        no = 4000 + i
        month, day = 1 + i // 50, 1 + i % 28
        if no == 4263:
            lines.append(f"2026-{month:02d}-{day:02d} 주문 #4263 고객사 다온전자, 품목 센서 모듈, 수량 1375, 담당 한")
            continue
        lines.append(
            f"2026-{month:02d}-{day:02d} 주문 #{no} 고객사 {rnd.choice(clients)}, 품목 {rnd.choice(['센서 모듈', '케이블', '전원부', '외함'])}, "
            f"수량 {rnd.randint(10, 900)}, 담당 {rnd.choice(staff)}"
        )
    return "\n".join(lines)


scenarios_3 = [
    {
        "id": "proj-chart-code",
        "note": "run_python의 code로 스크립트 작성과 실행을 한 번에 하는지 (이전 5~6회 → 목표 3회)",
        "mode": "project",
        "files": {"data/비용.csv": COSTS_CSV},
        "approve": ["run_python"],
        "turns": ["data/비용.csv로 항목별 비용 비율을 보여 주는 원형 차트 이미지를 charts/비용.png로 만들어 줘."],
        "checks": [
            {"kind": "toolCalled", "text": "run_python"},
            {"kind": "toolNotCalled", "text": "write_file"},
            {"kind": "fileExists", "path": "charts/비용.png"},
            {"kind": "maxCalls", "n": 3},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-docx-plan",
        "note": "Word 문서 작성: 분량 요구가 있어도 검사용 스크립트를 만들지 않는지 (제목별 글자 수 제공 효과)",
        "mode": "project",
        "files": {"notes/신사업.md": PLAN_NOTES},
        "approve": ["run_python"],
        "turns": [
            "notes/신사업.md를 바탕으로 사업 계획서를 Word 파일 docs/사업계획서.docx로 만들어 줘. "
            "1) 배경 2) 서비스 내용 3) 비용과 일정 4) 기대 효과 5) 위험 요소 순서로 쓰고, 각 장은 400자 이상, 비용과 일정은 표로 넣어 줘."
        ],
        "checks": [
            {"kind": "fileExists", "path": "docs/사업계획서.docx"},
            {"kind": "fileMinChars", "path": "docs/사업계획서.docx", "n": 2000},
            {"kind": "fileContains", "path": "docs/사업계획서.docx", "text": "|"},
            {"kind": "toolNotCalled", "text": "run_python"},
            {"kind": "maxCalls", "n": 3},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-pptx",
        "note": "발표 자료: 슬라이드 나누기와 발표자 노트 규칙을 지키는지",
        "mode": "project",
        "files": {"notes/신사업.md": PLAN_NOTES},
        "turns": ["notes/신사업.md 내용으로 임원 보고용 발표 자료 5장을 slides/신사업.pptx로 만들어 줘. 각 장에 발표자 노트도 넣어 줘."],
        "checks": [
            {"kind": "fileExists", "path": "slides/신사업.pptx"},
            {"kind": "fileContainsAny", "path": "slides/신사업.pptx", "text": "\n---|\n## "},
            {"kind": "fileContains", "path": "slides/신사업.pptx", "text": "노트:"},
            {"kind": "maxCalls", "n": 3},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-xlsx",
        "note": "엑셀을 Markdown 표로 만들기: 수식 칸과 시트 나누기 규칙",
        "mode": "project",
        "files": {"data/분기.csv": QUARTER_CSV},
        "turns": ["data/분기.csv로 엑셀 파일 reports/분기요약.xlsx를 만들어 줘. 분기마다 국내+해외 합계 열을 수식으로 넣고, 맨 아래에 연간 합계 행도 수식으로 넣어 줘."],
        "checks": [
            {"kind": "fileExists", "path": "reports/분기요약.xlsx"},
            {"kind": "fileContains", "path": "reports/분기요약.xlsx", "text": "=SUM("},
            {"kind": "toolNotCalled", "text": "run_python"},
            {"kind": "maxCalls", "n": 3},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-multiturn",
        "note": "한 프로젝트에서 이어지는 요청 3번: 앞에서 쓴 내용을 기억해 다시 읽지 않는지",
        "mode": "project",
        "files": {"notes/회의_1112.md": MINUTES},
        "turns": [
            "notes/회의_1112.md를 읽고 결정 사항과 기한을 표로 정리해서 summary.md로 저장해 줘.",
            "방금 만든 summary.md 맨 아래에 '다음 회의: 11월 20일 오전 10시' 한 줄을 추가해 줘.",
            "summary.md 전체를 영어로 번역해서 summary_en.md로 저장해 줘.",
        ],
        "checks": [
            {"kind": "fileContains", "path": "summary.md", "text": "11월 27일"},
            {"kind": "fileContains", "path": "summary.md", "text": "다음 회의: 11월 20일 오전 10시"},
            {"kind": "fileExists", "path": "summary_en.md"},
            {"kind": "fileContainsAny", "path": "summary_en.md", "text": "November 20|Nov 20|20 November"},
            {"kind": "maxCalls", "n": 8},
            {"kind": "noToolErrors"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "code-bugfix",
        "note": "코드 수정: 시험을 돌려 원인을 찾고, 고친 뒤 다시 돌려 확인하는지",
        "mode": "project",
        "files": {"calc.py": CALC_PY, "tests/test_calc.py": TEST_CALC_PY},
        "approve": ["run_python"],
        "turns": ["tests/test_calc.py를 실행하면 실패하는 항목이 있어. 원인을 찾아 calc.py를 고치고, 다시 실행해서 모두 통과하는지 확인해 줘."],
        "checks": [
            {"kind": "fileContains", "path": "calc.py", "text": "sum(values) / len(values)"},
            {"kind": "fileContains", "path": "calc.py", "text": "def discount(price, percent):"},
            {"kind": "toolCalled", "text": "run_python"},
            {"kind": "maxCalls", "n": 5},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-search",
        "note": "여러 파일에서 내용 찾기: search_files를 쓰는지, 파일을 하나씩 다 읽는지",
        "mode": "project",
        "files": POLICY_FILES,
        "turns": ["docs 폴더 문서들 중에서 환불할 때 금액이 깎이는 경우가 어디에 적혀 있는지 찾아서, 파일 이름과 내용을 알려 줘."],
        "checks": [
            {"kind": "answerContains", "text": "10%"},
            {"kind": "answerContains", "text": "정책_03_환불.md"},
            {"kind": "maxCalls", "n": 3},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-delete",
        "note": "삭제 승인 흐름: 여러 삭제를 한 응답에 모으는지, 남길 파일을 지키는지",
        "mode": "project",
        "files": {"보고서.md": "# 보고서\n", "임시1.tmp": "x", "임시2.tmp": "y", "old/백업.bak": "z"},
        "approve": ["delete_path"],
        "turns": ["확장자가 .tmp인 파일과 old 폴더를 지워 줘. 보고서.md는 그대로 둬."],
        "checks": [
            {"kind": "fileMissing", "path": "임시1.tmp"},
            {"kind": "fileMissing", "path": "임시2.tmp"},
            {"kind": "fileMissing", "path": "old"},
            {"kind": "fileExists", "path": "보고서.md"},
            {"kind": "maxCalls", "n": 2},
            {"kind": "finished"},
        ],
    },
    {
        "id": "chat-max-puzzle",
        "note": "추론 강도 '최대': 조건이 많은 문제에서 걸리는 시간과 정확도",
        "mode": "chat",
        "turns": [
            "다섯 팀(가, 나, 다, 라, 마)이 월요일부터 금요일까지 하루에 한 팀씩 발표합니다. 조건은 다음과 같습니다. "
            "① 가는 수요일보다 앞이다. ② 나는 가의 이틀 뒤다. ③ 다는 금요일이 아니다. ④ 라는 마보다 뒤다. ⑤ 마는 월요일이 아니다. "
            "⑥ 다와 마는 연속한 날이 아니다. ⑦ 다는 나보다 앞이다. ⑧ 다는 화요일이 아니다. "
            "요일별 발표 팀을 구하고, 마지막 줄에 '정답: 월-?, 화-?, 수-?, 목-?, 금-?' 형식으로 적어 주세요."
        ],
        "checks": [
            {"kind": "answerContainsAny", "text": "월-다, 화-가, 수-마, 목-나, 금-라|월-다,화-가,수-마,목-나,금-라"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "chat-long-input",
        "note": "긴 입력(주문 기록 520줄): 한가운데 정보를 찾는 정확도와 긴 입력의 응답 시간",
        "mode": "chat",
        "turns": [
            "아래는 주문 기록입니다.\n\n" + order_log() + "\n\n질문: 주문 #4263의 고객사, 수량, 담당자를 알려 주세요. 그리고 이 기록에서 담당자가 '한'인 주문은 모두 몇 건인가요?"
        ],
        "checks": [
            {"kind": "answerContains", "text": "다온전자"},
            {"kind": "answerContainsAny", "text": "1375|1,375"},
            {"kind": "answerContainsAny", "text": "1건|한 건|1 건"},
            {"kind": "finished"},
        ],
    },
]

# ---------------------------------------------------------------- 4차: 3차에서 고친 것 확인
SURVEY_CSV = """부서,만족도
영업,72
개발,81
디자인,77
인사,69
재무,74
"""

TODO_MD = """# 할 일 메모

- 고객사 견적서 보내기 (급함, 내일까지)
- 사내 교육 자료 고치기 (다음 주)
- 서버 점검 일정 잡기 (이번 주 금요일)
- 회의실 예약 규칙 정리 (여유 있음)
"""

TEXTUTIL_PY = """def word_count(text):
    # 낱말 수를 센다. 빈칸이 여러 개 이어져도 낱말 하나로 세지 않는다.
    return len(text.split(" "))


def shorten(text, limit):
    # limit 글자를 넘으면 잘라서 끝에 ... 을 붙인다.
    if len(text) <= limit:
        return text
    return text[:limit] + "..."
"""

TEST_TEXTUTIL_PY = """import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from textutil import shorten, word_count

failed = 0


def check(name, got, want):
    global failed
    if got != want:
        failed += 1
        print(f"FAIL {name}: got {got!r}, want {want!r}")
    else:
        print(f"ok   {name}")


check("three words", word_count("가 나 다"), 3)
check("double spaces", word_count("가  나   다"), 3)
check("short text", shorten("안녕", 5), "안녕")
check("long text", shorten("안녕하세요 반갑습니다", 5), "안녕하세요...")
print("failed:", failed)
sys.exit(1 if failed else 0)
"""

scenarios_4 = [
    {
        "id": "proj-chart-2",
        "note": "한글이 든 차트: 글꼴 설정을 앱이 해 주면 한 번에 끝나는지 (이전 9회 → 목표 3회)",
        "mode": "project",
        "files": {"data/만족도.csv": SURVEY_CSV},
        "approve": ["run_python"],
        "turns": ["data/만족도.csv로 부서별 만족도 가로 막대 차트 이미지를 charts/만족도.png로 만들어 줘. 제목과 축 이름은 한글로 넣어 줘."],
        "checks": [
            {"kind": "fileExists", "path": "charts/만족도.png"},
            {"kind": "toolNotCalled", "text": "write_file"},
            {"kind": "maxCalls", "n": 3},
            {"kind": "noToolErrors"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "proj-append",
        "note": "직접 쓴 파일 끝에 덧붙이기: 다시 읽지 않고 바로 덧붙이는지 (이전 3회 → 목표 2회)",
        "mode": "project",
        "files": {"notes/todo.md": TODO_MD},
        "turns": [
            "notes/todo.md를 읽고 급한 순서대로 번호를 매긴 표를 plan.md로 저장해 줘.",
            "plan.md 맨 끝에 '검토자: 품질팀' 한 줄을 추가해 줘.",
        ],
        "checks": [
            {"kind": "fileContains", "path": "plan.md", "text": "견적서"},
            {"kind": "fileContains", "path": "plan.md", "text": "검토자: 품질팀"},
            {"kind": "maxCalls", "n": 5},
            {"kind": "noToolErrors"},
            {"kind": "finished"},
        ],
    },
    {
        "id": "code-bugfix-2",
        "note": "코드 수정 후 확인 실행: python 명령으로 실행해도 앱의 Python 실행기로 돌아가는지",
        "mode": "project",
        "files": {"textutil.py": TEXTUTIL_PY, "tests/test_textutil.py": TEST_TEXTUTIL_PY},
        "approve": ["run_python", "run_command"],
        "turns": ["tests/test_textutil.py에 실패하는 항목이 있어. textutil.py를 고치고 시험을 실행해서 모두 통과하는지 확인해 줘."],
        "checks": [
            {"kind": "fileContains", "path": "textutil.py", "text": "text.split()"},
            {"kind": "answerContainsAny", "text": "통과|failed: 0|모두"},
            {"kind": "maxCalls", "n": 4},
            {"kind": "finished"},
        ],
    },
]

out = pathlib.Path(__file__).with_name("scenarios.json")
out.write_text(json.dumps(scenarios, ensure_ascii=False, indent=2), encoding="utf-8")
print(f"{len(scenarios)}개 시나리오 → {out}")
out2 = pathlib.Path(__file__).with_name("scenarios_2.json")
out2.write_text(json.dumps(scenarios_2, ensure_ascii=False, indent=2), encoding="utf-8")
print(f"{len(scenarios_2)}개 시나리오 → {out2}")
out3 = pathlib.Path(__file__).with_name("scenarios_3.json")
out3.write_text(json.dumps(scenarios_3, ensure_ascii=False, indent=2), encoding="utf-8")
print(f"{len(scenarios_3)}개 시나리오 → {out3}")
out4 = pathlib.Path(__file__).with_name("scenarios_4.json")
out4.write_text(json.dumps(scenarios_4, ensure_ascii=False, indent=2), encoding="utf-8")
print(f"{len(scenarios_4)}개 시나리오 → {out4}")
