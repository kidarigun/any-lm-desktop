import {
  File,
  FileCode2,
  FileImage,
  FileSpreadsheet,
  FileText,
  Folder,
  FolderOpen,
  Presentation,
} from "lucide-react";
import { extensionOf } from "../docs/formats";

const CODE_EXT = new Set([
  "ts", "tsx", "js", "jsx", "mjs", "cjs", "json", "rs", "py", "java", "kt", "go", "c", "h", "cpp", "hpp",
  "cs", "css", "scss", "html", "htm", "xml", "yml", "yaml", "toml", "sh", "ps1", "sql", "vue", "svelte",
]);
const IMAGE_EXT = new Set(["png", "jpg", "jpeg", "gif", "bmp", "svg", "webp", "ico"]);
const SHEET_EXT = new Set(["xlsx", "xls", "csv", "tsv"]);

export function FileIcon({ name, isDir, open }: { name: string; isDir: boolean; open?: boolean }) {
  const cls = "w-3.5 h-3.5 shrink-0";
  if (isDir) {
    return open ? (
      <FolderOpen className={`${cls} text-amber-600 dark:text-amber-400`} />
    ) : (
      <Folder className={`${cls} text-amber-600 dark:text-amber-400`} />
    );
  }
  const ext = extensionOf(name);
  if (ext === "md" || ext === "markdown" || ext === "txt") return <FileText className={`${cls} text-stone-500 dark:text-stone-400`} />;
  if (ext === "docx" || ext === "doc" || ext === "hwp" || ext === "hwpx") return <FileText className={`${cls} text-blue-600 dark:text-blue-400`} />;
  if (ext === "pptx" || ext === "ppt") return <Presentation className={`${cls} text-orange-600 dark:text-orange-400`} />;
  if (ext === "pdf") return <FileText className={`${cls} text-red-600 dark:text-red-400`} />;
  if (SHEET_EXT.has(ext)) return <FileSpreadsheet className={`${cls} text-emerald-600 dark:text-emerald-400`} />;
  if (IMAGE_EXT.has(ext)) return <FileImage className={`${cls} text-violet-600 dark:text-violet-400`} />;
  if (CODE_EXT.has(ext)) return <FileCode2 className={`${cls} text-sky-600 dark:text-sky-400`} />;
  return <File className={`${cls} text-stone-400`} />;
}
