import { EditorContent, useEditor, useEditorState, type Editor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Image from "@tiptap/extension-image";
import { TableKit } from "@tiptap/extension-table";
import {
  Bold,
  Code,
  Heading1,
  Heading2,
  Heading3,
  Italic,
  Link2,
  List,
  ListOrdered,
  Minus,
  Pilcrow,
  Quote,
  Redo2,
  SquareCode,
  Strikethrough,
  Table as TableIcon,
  Underline as UnderlineIcon,
  Undo2,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

export interface RichTextEditorProps {
  initialHtml: string;
  onChange: (html: string) => void;
  /** "page": Word처럼 종이 모양, "plain": 문서 패널 폭에 맞춘 Markdown 편집 */
  variant: "page" | "plain";
  onReady?: (editor: Editor) => void;
}

export function RichTextEditor({ initialHtml, onChange, variant, onReady }: RichTextEditorProps) {
  const editor = useEditor({
    extensions: [
      StarterKit.configure({ link: { openOnClick: false, autolink: true } }),
      TableKit.configure({ table: { resizable: false } }),
      Image.configure({ allowBase64: true, inline: false }),
    ],
    content: initialHtml,
    onUpdate: ({ editor }) => onChange(editor.getHTML()),
    editorProps: {
      attributes: {
        class: `canvas-prose focus:outline-none ${variant === "page" ? "canvas-page" : "canvas-plain"}`,
        spellcheck: "false",
      },
    },
  });

  useEffect(() => {
    if (editor && onReady) onReady(editor);
  }, [editor, onReady]);

  if (!editor) return null;
  return (
    <div className="flex flex-col h-full min-h-0">
      <Toolbar editor={editor} />
      <div
        className={`flex-1 overflow-y-auto ${
          variant === "page" ? "bg-stone-200/70 dark:bg-[#161614] py-6 px-4" : "px-6 py-5"
        }`}
      >
        <EditorContent editor={editor} />
      </div>
    </div>
  );
}

function ToolButton({
  onClick,
  active,
  disabled,
  title,
  children,
}: {
  onClick: () => void;
  active?: boolean;
  disabled?: boolean;
  title: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={title}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
      disabled={disabled}
      className={`p-1.5 rounded-md cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed ${
        active
          ? "bg-[#A50034]/10 text-[#A50034] dark:bg-[#FF5C8D]/15 dark:text-[#FF5C8D]"
          : "text-stone-600 dark:text-stone-300 hover:bg-stone-200 dark:hover:bg-[#32322F]"
      }`}
    >
      {children}
    </button>
  );
}

function Divider() {
  return <span className="w-px h-5 bg-stone-300 dark:bg-[#3A3A36] mx-0.5" />;
}

function Toolbar({ editor }: { editor: Editor }) {
  const s = useEditorState({
    editor,
    selector: ({ editor: e }) => ({
      bold: e.isActive("bold"),
      italic: e.isActive("italic"),
      underline: e.isActive("underline"),
      strike: e.isActive("strike"),
      code: e.isActive("code"),
      h1: e.isActive("heading", { level: 1 }),
      h2: e.isActive("heading", { level: 2 }),
      h3: e.isActive("heading", { level: 3 }),
      bullet: e.isActive("bulletList"),
      ordered: e.isActive("orderedList"),
      quote: e.isActive("blockquote"),
      codeBlock: e.isActive("codeBlock"),
      link: e.isActive("link"),
      table: e.isActive("table"),
      canUndo: e.can().undo(),
      canRedo: e.can().redo(),
    }),
  });
  const [linkOpen, setLinkOpen] = useState(false);
  const [linkUrl, setLinkUrl] = useState("");
  const chain = () => editor.chain().focus();

  const applyLink = () => {
    const url = linkUrl.trim();
    if (!url) chain().extendMarkRange("link").unsetLink().run();
    else chain().extendMarkRange("link").setLink({ href: /^[a-z]+:/i.test(url) ? url : `https://${url}` }).run();
    setLinkOpen(false);
  };

  return (
    <div className="border-b border-[#E5E3DB] dark:border-[#343430] bg-[#FAF9F5] dark:bg-[#1E1E1C]">
      <div className="flex flex-wrap items-center gap-0.5 px-2 py-1.5">
        <ToolButton title="실행 취소 (Ctrl+Z)" disabled={!s.canUndo} onClick={() => chain().undo().run()}>
          <Undo2 className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="다시 실행 (Ctrl+Y)" disabled={!s.canRedo} onClick={() => chain().redo().run()}>
          <Redo2 className="w-3.5 h-3.5" />
        </ToolButton>
        <Divider />
        <ToolButton title="본문" onClick={() => chain().setParagraph().run()} active={!s.h1 && !s.h2 && !s.h3}>
          <Pilcrow className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="제목 1" active={s.h1} onClick={() => chain().toggleHeading({ level: 1 }).run()}>
          <Heading1 className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="제목 2" active={s.h2} onClick={() => chain().toggleHeading({ level: 2 }).run()}>
          <Heading2 className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="제목 3" active={s.h3} onClick={() => chain().toggleHeading({ level: 3 }).run()}>
          <Heading3 className="w-3.5 h-3.5" />
        </ToolButton>
        <Divider />
        <ToolButton title="굵게 (Ctrl+B)" active={s.bold} onClick={() => chain().toggleBold().run()}>
          <Bold className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="기울임 (Ctrl+I)" active={s.italic} onClick={() => chain().toggleItalic().run()}>
          <Italic className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="밑줄 (Ctrl+U)" active={s.underline} onClick={() => chain().toggleUnderline().run()}>
          <UnderlineIcon className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="취소선" active={s.strike} onClick={() => chain().toggleStrike().run()}>
          <Strikethrough className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="인라인 코드" active={s.code} onClick={() => chain().toggleCode().run()}>
          <Code className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton
          title="링크"
          active={s.link || linkOpen}
          onClick={() => {
            setLinkUrl((editor.getAttributes("link").href as string) || "");
            setLinkOpen((v) => !v);
          }}
        >
          <Link2 className="w-3.5 h-3.5" />
        </ToolButton>
        <Divider />
        <ToolButton title="글머리 기호 목록" active={s.bullet} onClick={() => chain().toggleBulletList().run()}>
          <List className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="번호 목록" active={s.ordered} onClick={() => chain().toggleOrderedList().run()}>
          <ListOrdered className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="인용" active={s.quote} onClick={() => chain().toggleBlockquote().run()}>
          <Quote className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="코드 블록" active={s.codeBlock} onClick={() => chain().toggleCodeBlock().run()}>
          <SquareCode className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton title="구분선" onClick={() => chain().setHorizontalRule().run()}>
          <Minus className="w-3.5 h-3.5" />
        </ToolButton>
        <ToolButton
          title="표 삽입 (3×3)"
          active={s.table}
          onClick={() => chain().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()}
        >
          <TableIcon className="w-3.5 h-3.5" />
        </ToolButton>
      </div>

      {linkOpen && (
        <div className="flex items-center gap-1.5 px-2 pb-2 text-xs">
          <input
            autoFocus
            value={linkUrl}
            onChange={(e) => setLinkUrl(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") applyLink();
              if (e.key === "Escape") setLinkOpen(false);
            }}
            placeholder="https://… (비우면 링크 제거)"
            className="flex-1 bg-white dark:bg-[#262624] border border-stone-300 dark:border-[#3A3A36] rounded-lg px-2 py-1 text-stone-900 dark:text-stone-100 focus:outline-none focus:border-[#A50034]"
          />
          <button onClick={applyLink} className="px-2.5 py-1 rounded-lg bg-[#A50034] text-white font-semibold cursor-pointer">
            적용
          </button>
        </div>
      )}

      {s.table && (
        <div className="flex flex-wrap items-center gap-1 px-2 pb-1.5 text-xs">
          <span className="text-stone-500 dark:text-stone-400 mr-1">표:</span>
          {[
            ["위에 행", () => chain().addRowBefore().run()],
            ["아래에 행", () => chain().addRowAfter().run()],
            ["왼쪽에 열", () => chain().addColumnBefore().run()],
            ["오른쪽에 열", () => chain().addColumnAfter().run()],
            ["행 삭제", () => chain().deleteRow().run()],
            ["열 삭제", () => chain().deleteColumn().run()],
            ["표 삭제", () => chain().deleteTable().run()],
          ].map(([label, fn]) => (
            <button
              key={label as string}
              onMouseDown={(e) => e.preventDefault()}
              onClick={fn as () => void}
              className="px-2 py-0.5 rounded-md bg-stone-200/80 dark:bg-[#2C2C29] hover:bg-stone-300 dark:hover:bg-[#383834] text-stone-700 dark:text-stone-200 cursor-pointer"
            >
              {label as string}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
