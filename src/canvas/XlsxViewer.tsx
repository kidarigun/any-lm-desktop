import { useEffect, useState } from "react";
import { columnName, readXlsxSheets, type XlsxSheetData } from "../docs/xlsx";

/** 미리보기에서 보여 줄 최대 행·열 */
const VIEW_LIMITS = { maxRows: 2000, maxCols: 100 };

/** 엑셀 통합 문서를 시트별 표로 보여 준다 (읽기 전용). */
export function XlsxViewer({ bytes, onError }: { bytes: Uint8Array; onError: (msg: string) => void }) {
  const [sheets, setSheets] = useState<XlsxSheetData[] | null>(null);
  const [current, setCurrent] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setSheets(null);
    setCurrent(0);
    readXlsxSheets(bytes, VIEW_LIMITS)
      .then((s) => !cancelled && setSheets(s))
      .catch((e) => {
        if (cancelled) return;
        setSheets([]);
        onError(`엑셀 파일을 읽지 못했습니다: ${e instanceof Error ? e.message : String(e)}`);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bytes]);

  if (!sheets) return <div className="p-6 text-xs text-stone-500">시트를 읽는 중...</div>;
  if (sheets.length === 0) return <div className="p-6 text-xs text-stone-500">시트가 없습니다.</div>;

  const sheet = sheets[Math.min(current, sheets.length - 1)];
  // 빈 시트도 엑셀처럼 칸이 보이도록 최소 크기를 둔다.
  const cols = Math.max(sheet.width, 6);
  const rows = Math.max(sheet.rows.length, 20);

  return (
    <div className="flex flex-col h-full min-h-0 bg-white dark:bg-[#1E1E1C]">
      <div className="flex-1 min-h-0 overflow-auto" data-testid="xlsx-grid">
        <table className="border-collapse text-xs text-stone-800 dark:text-stone-100" style={{ tableLayout: "auto" }}>
          <thead>
            <tr>
              <th className="sticky top-0 left-0 z-20 w-10 min-w-10 bg-[#F0EFEA] dark:bg-[#2A2A27] border border-stone-300 dark:border-[#3A3A36]" />
              {Array.from({ length: cols }, (_, c) => (
                <th
                  key={c}
                  className="sticky top-0 z-10 min-w-[72px] px-2 py-1 font-semibold text-stone-500 dark:text-stone-400 bg-[#F0EFEA] dark:bg-[#2A2A27] border border-stone-300 dark:border-[#3A3A36]"
                >
                  {columnName(c)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {Array.from({ length: rows }, (_, r) => (
              <tr key={r}>
                <th className="sticky left-0 z-10 px-1.5 py-1 font-semibold text-right text-stone-500 dark:text-stone-400 bg-[#F0EFEA] dark:bg-[#2A2A27] border border-stone-300 dark:border-[#3A3A36] tabular-nums">
                  {r + 1}
                </th>
                {Array.from({ length: cols }, (_, c) => {
                  const cell = sheet.rows[r]?.[c];
                  // 계산 결과가 저장되지 않은 수식(모델이 만든 파일 등)은 Excel에서 열 때 계산된다.
                  const uncomputed = !!cell?.formula && cell.display === `=${cell.formula}`;
                  return (
                    <td
                      key={c}
                      title={
                        cell?.formula
                          ? uncomputed
                            ? `수식 =${cell.formula} (Excel에서 열면 계산됩니다)`
                            : `수식 =${cell.formula}`
                          : undefined
                      }
                      className={`px-2 py-1 border border-stone-200 dark:border-[#34342F] whitespace-pre max-w-[320px] overflow-hidden text-ellipsis ${
                        cell?.numeric ? "text-right tabular-nums" : ""
                      } ${uncomputed ? "italic text-[#1D6F42] dark:text-[#6FCF97]" : ""}`}
                    >
                      {cell?.display ?? ""}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
        {sheet.truncated && (
          <div className="px-3 py-2 text-[11px] text-stone-500">
            시트가 커서 {VIEW_LIMITS.maxRows}행·{VIEW_LIMITS.maxCols}열까지만 보여 줍니다. 전체는 Excel에서 여세요.
          </div>
        )}
      </div>

      {/* 시트 탭 */}
      <div className="flex items-center gap-0.5 px-2 py-1 border-t border-[#E5E3DB] dark:border-[#343430] bg-[#F6F5F0] dark:bg-[#1B1B19] overflow-x-auto shrink-0" role="tablist">
        {sheets.map((s, i) => (
          <button
            key={`${i}:${s.name}`}
            role="tab"
            aria-selected={i === current}
            onClick={() => setCurrent(i)}
            className={`px-3 py-1 rounded-md text-xs whitespace-nowrap cursor-pointer ${
              i === current
                ? "bg-white dark:bg-[#32322E] font-semibold text-[#1D6F42] dark:text-[#6FCF97] shadow-2xs"
                : "text-stone-600 dark:text-stone-400 hover:bg-stone-200 dark:hover:bg-[#2C2C29]"
            }`}
          >
            {s.name}
          </button>
        ))}
      </div>
    </div>
  );
}
