import { useCallback, useEffect, useRef, useState } from "react";
import {
  useEditor,
  EditorContent,
  NodeViewWrapper,
  NodeViewContent,
  ReactNodeViewRenderer,
} from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import CodeBlockExtension from "@tiptap/extension-code-block";
import { Image } from "@tiptap/extension-image";
import { Table } from "@tiptap/extension-table";
import { TableRow } from "@tiptap/extension-table-row";
import { TableCell } from "@tiptap/extension-table-cell";
import { TableHeader } from "@tiptap/extension-table-header";
import Placeholder from "@tiptap/extension-placeholder";
import { Markdown } from "tiptap-markdown";
import type { NodeViewProps } from "@tiptap/react";
import type { Icon, IconWeight } from "@phosphor-icons/react";
import {
  TextBIcon,
  TextItalicIcon,
  TextStrikethroughIcon,
  ListBulletsIcon,
  ListNumbersIcon,
  QuotesIcon,
  CodeIcon,
  CodeBlockIcon,
  TextHOneIcon,
  TextHTwoIcon,
  TextHThreeIcon,
  ArrowCounterClockwiseIcon,
  ArrowClockwiseIcon,
  MinusIcon,
  TableIcon,
  RowsPlusTopIcon,
  RowsPlusBottomIcon,
  ColumnsPlusLeftIcon,
  ColumnsPlusRightIcon,
  RowsIcon,
  ColumnsIcon,
  TrashIcon,
  ImageIcon,
  FloppyDiskIcon,
  ArrowLeftIcon,
  CircleNotchIcon,
  EyeIcon,
  PencilSimpleIcon,
} from "@phosphor-icons/react";
import type { Editor } from "@tiptap/react";
import { MarkdownPreview } from "./MarkdownPreview";
import { ImageInsertDialog, type ImageInsertDialogHandle } from "./ImageInsertDialog";

// ---------------------------------------------------------------------------
// Code block with editable language label
// ---------------------------------------------------------------------------

function CodeBlockView({ node, updateAttributes, extension }: Readonly<NodeViewProps>) {
  const language = (node.attrs as { language?: string }).language ?? "";

  return (
    <NodeViewWrapper className="relative">
      <input
        type="text"
        value={language}
        onChange={(e) => updateAttributes({ language: e.target.value })}
        placeholder={(extension.options as { defaultLanguage?: string }).defaultLanguage ?? "plain"}
        contentEditable={false}
        className="text-micro text-text-muted bg-base-200/80 absolute top-1.5 right-2 z-10 w-20 rounded border-none px-1.5 py-0.5 text-right outline-none"
        aria-label="Code block language"
      />
      <pre>
        <code>
          <NodeViewContent />
        </code>
      </pre>
    </NodeViewWrapper>
  );
}

const CodeBlockWithLanguage = CodeBlockExtension.extend({
  addNodeView() {
    return ReactNodeViewRenderer(CodeBlockView);
  },
});

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface MarkdownEditorProps {
  readonly initialContent: string;
  readonly documentName: string;
  readonly onSave: (content: string) => Promise<void>;
  readonly onClose: () => void;
  /** Called when dirty state changes so the parent can track unsaved state. */
  readonly onDirtyChange?: (dirty: boolean) => void;
  readonly saving: boolean;
  /**
   * If provided, the title becomes an editable input. Called with the new
   * filename (including `.md`) on Enter / blur. Omit for read-only display.
   */
  readonly onRename?: (newFilename: string) => void;
}

const MD_EXTENSION = ".md";

function stripMdExtension(name: string): string {
  return name.endsWith(MD_EXTENSION) ? name.slice(0, -MD_EXTENSION.length) : name;
}

// ---------------------------------------------------------------------------
// Toolbar button
// ---------------------------------------------------------------------------

interface ToolbarButtonProps {
  readonly icon: Icon;
  readonly label: string;
  readonly active?: boolean;
  readonly disabled?: boolean;
  readonly onClick: () => void;
}

function ToolbarButton({
  icon: IconComponent,
  label,
  active,
  disabled,
  onClick,
}: ToolbarButtonProps) {
  const weight: IconWeight = active ? "bold" : "regular";
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      aria-pressed={active}
      className={`btn btn-ghost btn-xs btn-square rounded-md ${active ? "bg-base-300" : ""}`}
    >
      <IconComponent size={15} weight={weight} />
    </button>
  );
}

function ToolbarDivider() {
  return <div className="bg-base-300 mx-0.5 h-4 w-px" />;
}

// ---------------------------------------------------------------------------
// Toolbar
// ---------------------------------------------------------------------------

interface EditorToolbarProps {
  readonly editor: Editor;
  readonly onInsertImage: () => void;
}

function EditorToolbar({ editor, onInsertImage }: EditorToolbarProps) {
  return (
    <div className="flex flex-wrap items-center gap-0.5" role="toolbar" aria-label="Formatting">
      <ToolbarButton
        icon={TextBIcon}
        label="Bold"
        active={editor.isActive("bold")}
        onClick={() => editor.chain().focus().toggleBold().run()}
      />
      <ToolbarButton
        icon={TextItalicIcon}
        label="Italic"
        active={editor.isActive("italic")}
        onClick={() => editor.chain().focus().toggleItalic().run()}
      />
      <ToolbarButton
        icon={TextStrikethroughIcon}
        label="Strikethrough"
        active={editor.isActive("strike")}
        onClick={() => editor.chain().focus().toggleStrike().run()}
      />
      <ToolbarButton
        icon={CodeIcon}
        label="Inline code"
        active={editor.isActive("code")}
        onClick={() => editor.chain().focus().toggleCode().run()}
      />

      <ToolbarDivider />

      <ToolbarButton
        icon={TextHOneIcon}
        label="Heading 1"
        active={editor.isActive("heading", { level: 1 })}
        onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}
      />
      <ToolbarButton
        icon={TextHTwoIcon}
        label="Heading 2"
        active={editor.isActive("heading", { level: 2 })}
        onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
      />
      <ToolbarButton
        icon={TextHThreeIcon}
        label="Heading 3"
        active={editor.isActive("heading", { level: 3 })}
        onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
      />

      <ToolbarDivider />

      <ToolbarButton
        icon={ListBulletsIcon}
        label="Bullet list"
        active={editor.isActive("bulletList")}
        onClick={() => editor.chain().focus().toggleBulletList().run()}
      />
      <ToolbarButton
        icon={ListNumbersIcon}
        label="Ordered list"
        active={editor.isActive("orderedList")}
        onClick={() => editor.chain().focus().toggleOrderedList().run()}
      />
      <ToolbarButton
        icon={QuotesIcon}
        label="Blockquote"
        active={editor.isActive("blockquote")}
        onClick={() => editor.chain().focus().toggleBlockquote().run()}
      />
      <ToolbarButton
        icon={CodeBlockIcon}
        label="Code block"
        active={editor.isActive("codeBlock")}
        onClick={() => editor.chain().focus().toggleCodeBlock().run()}
      />
      <ToolbarButton
        icon={MinusIcon}
        label="Horizontal rule"
        onClick={() => editor.chain().focus().setHorizontalRule().run()}
      />
      <ToolbarButton
        icon={TableIcon}
        label="Insert table"
        onClick={() =>
          editor.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()
        }
      />
      <ToolbarButton icon={ImageIcon} label="Insert image" onClick={onInsertImage} />

      <ToolbarDivider />

      <ToolbarButton
        icon={ArrowCounterClockwiseIcon}
        label="Undo"
        disabled={!editor.can().undo()}
        onClick={() => editor.chain().focus().undo().run()}
      />
      <ToolbarButton
        icon={ArrowClockwiseIcon}
        label="Redo"
        disabled={!editor.can().redo()}
        onClick={() => editor.chain().focus().redo().run()}
      />
    </div>
  );
}

function TableToolbar({ editor }: { readonly editor: Editor }) {
  return (
    <div className="flex items-center gap-0.5" role="toolbar" aria-label="Table editing">
      <span className="text-caption text-text-muted mr-1">Table:</span>
      <ToolbarButton
        icon={RowsPlusTopIcon}
        label="Add row above"
        onClick={() => editor.chain().focus().addRowBefore().run()}
      />
      <ToolbarButton
        icon={RowsPlusBottomIcon}
        label="Add row below"
        onClick={() => editor.chain().focus().addRowAfter().run()}
      />
      <ToolbarButton
        icon={ColumnsPlusLeftIcon}
        label="Add column left"
        onClick={() => editor.chain().focus().addColumnBefore().run()}
      />
      <ToolbarButton
        icon={ColumnsPlusRightIcon}
        label="Add column right"
        onClick={() => editor.chain().focus().addColumnAfter().run()}
      />

      <ToolbarDivider />

      <button
        type="button"
        onClick={() => editor.chain().focus().deleteRow().run()}
        className="btn btn-ghost btn-xs text-error/70 hover:text-error rounded-md"
      >
        <RowsIcon size={13} />
        <span className="text-micro">Delete row</span>
      </button>
      <button
        type="button"
        onClick={() => editor.chain().focus().deleteColumn().run()}
        className="btn btn-ghost btn-xs text-error/70 hover:text-error rounded-md"
      >
        <ColumnsIcon size={13} />
        <span className="text-micro">Delete col</span>
      </button>
      <button
        type="button"
        onClick={() => editor.chain().focus().deleteTable().run()}
        className="btn btn-ghost btn-xs text-error/70 hover:text-error rounded-md"
      >
        <TrashIcon size={13} />
        <span className="text-micro">Delete table</span>
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

export function MarkdownEditor({
  initialContent,
  documentName,
  onSave,
  onClose,
  onDirtyChange,
  saving,
  onRename,
}: MarkdownEditorProps) {
  const [dirty, setDirtyRaw] = useState(false);
  const [titleDraft, setTitleDraft] = useState(stripMdExtension(documentName));

  // Sync the input when the parent reports a new documentName (rename echo
  // from the peer, rename-from-content on save, etc.) — but only if the user
  // isn't mid-edit, to avoid clobbering their keystrokes.
  const titleInputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (document.activeElement === titleInputRef.current) return;
    setTitleDraft(stripMdExtension(documentName));
  }, [documentName]);

  const commitTitle = useCallback(() => {
    if (!onRename) return;
    // Strip trailing .md before comparing and before re-appending — a user who
    // types the full "foo.md" would otherwise produce "foo.md.md".
    const trimmed = stripMdExtension(titleDraft.trim());
    const current = stripMdExtension(documentName);
    if (!trimmed) {
      // Empty input: revert to the current name.
      setTitleDraft(current);
      return;
    }
    if (trimmed === current) return;
    onRename(trimmed + MD_EXTENSION);
  }, [titleDraft, documentName, onRename]);

  const handleTitleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter") {
        e.preventDefault();
        e.currentTarget.blur();
      } else if (e.key === "Escape") {
        e.preventDefault();
        setTitleDraft(stripMdExtension(documentName));
        e.currentTarget.blur();
      }
    },
    [documentName],
  );

  const setDirty = useCallback(
    (value: boolean) => {
      setDirtyRaw(value);
      onDirtyChange?.(value);
    },
    [onDirtyChange],
  );

  const [previewMarkdown, setPreviewMarkdown] = useState(initialContent);
  const [previewing, setPreviewing] = useState(false);
  const [inTable, setInTable] = useState(false);
  const lastSavedRef = useRef(initialContent);
  const containerRef = useRef<HTMLDivElement>(null);
  const blurTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const imageDialogRef = useRef<ImageInsertDialogHandle>(null);

  const getMarkdown = useCallback((editor: Editor): string => {
    const storage = editor.storage as { markdown?: { getMarkdown: () => string } };
    return storage.markdown?.getMarkdown() ?? "";
  }, []);

  const handleSave = useCallback(
    async (editor: Editor) => {
      const markdown = getMarkdown(editor);
      await onSave(markdown);
      lastSavedRef.current = markdown;
      setDirty(false);
    },
    [onSave, getMarkdown, setDirty],
  );

  const editor = useEditor({
    extensions: [
      StarterKit.configure({ codeBlock: false }),
      CodeBlockWithLanguage,
      Image.configure({ inline: false, allowBase64: false }),
      Table.configure({ resizable: false }),
      TableRow,
      TableCell,
      TableHeader,
      Placeholder.configure({ placeholder: "Start writing..." }),
      Markdown.configure({
        html: false,
        transformPastedText: true,
        transformCopiedText: true,
      }),
    ],
    content: initialContent,
    editorProps: {
      attributes: {
        class: "prose prose-editor prose-sm max-w-none focus:outline-none min-h-full p-6",
        "aria-label": "Document editor",
      },
      handleDoubleClickOn: (_view, _pos, node) => {
        if (node.type.name === "image") {
          const attrs = node.attrs as { src?: string; alt?: string };
          imageDialogRef.current?.show({
            url: attrs.src ?? "",
            alt: attrs.alt ?? "",
          });
          return true;
        }
        return false;
      },
    },
    onUpdate: ({ editor: e }) => {
      const current = getMarkdown(e);
      setDirty(current !== lastSavedRef.current);
      setPreviewMarkdown(current);
      const tableActive = e.isActive("table");
      setInTable((prev) => (prev === tableActive ? prev : tableActive));
    },
    onSelectionUpdate: ({ editor: e }) => {
      const tableActive = e.isActive("table");
      setInTable((prev) => (prev === tableActive ? prev : tableActive));
    },
  });

  // Debounced auto-preview: if focus leaves the editor container for 3s, switch to preview.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handleFocusIn = () => {
      if (blurTimerRef.current !== null) {
        clearTimeout(blurTimerRef.current);
        blurTimerRef.current = null;
      }
    };

    const handleFocusOut = (e: FocusEvent) => {
      // Still inside the container (toolbar, save button, etc.) — ignore.
      if (e.relatedTarget instanceof Node && container.contains(e.relatedTarget)) return;

      blurTimerRef.current = setTimeout(() => {
        // Final check: if focus returned to the container since the timer started, bail.
        if (!container.contains(document.activeElement)) {
          setPreviewing(true);
        }
        blurTimerRef.current = null;
      }, 3000);
    };

    container.addEventListener("focusin", handleFocusIn);
    container.addEventListener("focusout", handleFocusOut);
    return () => {
      container.removeEventListener("focusin", handleFocusIn);
      container.removeEventListener("focusout", handleFocusOut);
      if (blurTimerRef.current !== null) clearTimeout(blurTimerRef.current);
    };
  }, []);

  // Keyboard shortcuts: Escape → preview, Cmd/Ctrl+S → save
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !previewing) {
        setPreviewing(true);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "s") {
        e.preventDefault();
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- editor is captured by closure, can be null
        if (dirty && !saving && editor) {
          void handleSave(editor);
        }
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [editor, dirty, saving, handleSave, previewing]);

  // Warn on unsaved changes before leaving.
  useEffect(() => {
    if (!dirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [dirty]);

  const switchToEdit = useCallback(() => {
    // Set state first so the EditorContent mounts, then focus it.
    setPreviewing(false);
  }, []);

  // When leaving preview mode, focus the editor once it's mounted.
  useEffect(() => {
    if (!previewing) {
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- editor captured by closure, can be null
      requestAnimationFrame(() => editor?.commands.focus());
    }
  }, [previewing, editor]);

  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- useEditor returns Editor | null
  if (!editor) return null;

  return (
    <div className="flex h-full flex-col" ref={containerRef}>
      {/* Header bar */}
      <div className="border-base-300/50 flex shrink-0 items-center gap-3 border-b px-4 py-2">
        <button
          type="button"
          onClick={onClose}
          className="btn btn-ghost btn-sm btn-square rounded-md"
          aria-label="Close editor"
        >
          <ArrowLeftIcon size={16} />
        </button>

        <div className="flex min-w-0 flex-1 items-center gap-1.5">
          {onRename ? (
            <input
              ref={titleInputRef}
              type="text"
              value={titleDraft}
              onChange={(e) => setTitleDraft(e.target.value)}
              onBlur={commitTitle}
              onKeyDown={handleTitleKeyDown}
              placeholder="Untitled note"
              spellCheck={false}
              aria-label="Document title"
              className="text-ui text-base-content min-w-0 flex-1 truncate border-none bg-transparent px-0 outline-none focus:ring-0"
            />
          ) : (
            <span className="text-ui text-base-content min-w-0 flex-1 truncate">
              {documentName}
            </span>
          )}
          {dirty && (
            <span className="text-warning shrink-0 text-xs" aria-label="Unsaved changes">
              ●
            </span>
          )}
        </div>

        {/* Preview / Edit toggle */}
        <button
          type="button"
          onClick={() => (previewing ? switchToEdit() : setPreviewing(true))}
          className="btn btn-ghost btn-sm btn-square rounded-md"
          aria-label={previewing ? "Switch to editor" : "Switch to preview"}
        >
          {previewing ? <PencilSimpleIcon size={14} /> : <EyeIcon size={14} />}
        </button>

        <button
          type="button"
          onClick={() => void handleSave(editor)}
          disabled={!dirty || saving}
          className="btn btn-primary btn-sm rounded-lg text-xs"
        >
          {saving ? (
            <CircleNotchIcon size={14} className="animate-spin" />
          ) : (
            <FloppyDiskIcon size={14} />
          )}
          Save
        </button>
      </div>

      {/* Formatting toolbar — only visible in edit mode */}
      {!previewing && (
        <div className="border-base-300/50 flex shrink-0 flex-wrap items-center gap-0.5 border-b px-4 py-1.5">
          <EditorToolbar editor={editor} onInsertImage={() => imageDialogRef.current?.show()} />
          {inTable && (
            <>
              <ToolbarDivider />
              <TableToolbar editor={editor} />
            </>
          )}
        </div>
      )}

      {/* Single pane: edit or preview */}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {previewing ? (
          <div
            onClick={switchToEdit}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") switchToEdit();
            }}
            className="min-h-full cursor-text"
            aria-label="Click to edit"
          >
            <MarkdownPreview content={previewMarkdown} editorStyle />
          </div>
        ) : (
          <EditorContent editor={editor} className="h-full" />
        )}
      </div>

      <ImageInsertDialog
        ref={imageDialogRef}
        onInsert={(url, alt) => {
          // If the selection is on an image node, update it. Otherwise insert new.
          const { selection } = editor.state;
          const selectedNode =
            "node" in selection
              ? (selection as unknown as { node: { type: { name: string } } }).node
              : null;
          if (selectedNode?.type.name === "image") {
            editor.chain().focus().updateAttributes("image", { src: url, alt }).run();
          } else {
            editor.chain().focus().setImage({ src: url, alt }).run();
          }
        }}
      />
    </div>
  );
}
