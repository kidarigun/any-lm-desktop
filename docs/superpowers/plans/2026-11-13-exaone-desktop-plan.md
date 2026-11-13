# EXAONE Desktop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Claude Desktop + Claude Code benchmarked local desktop application using Tauri v2 (Rust + React/TypeScript) connected to the K-EXAONE 2.5 API (`https://api.exaone.ai/v1/chat/completions`), supporting local folder projects, direct file Read/Write/Modify/Delete, Undo snapshots, terminal execution with user confirmation, and a live Artifacts/Code Studio panel.

**Architecture:**
- **Rust Backend (`src-tauri`)**: Manages persistent state (`AppData` JSON store for settings, projects, conversations, and file snapshots), implements safe local workspace file operations (`list_dir`, `read_file`, `write_file`, `modify_file`, `delete_path`, `search_files`), shell execution (`run_command`), and an autonomous K-EXAONE 2.5 streaming + tool-calling client that auto-executes Read/Write/Modify tools and requests user approval for Delete/Shell tools.
- **React Frontend (`src`)**: Provides a 3-pane Claude-inspired interface (Left: Project switcher, System prompt, Live Local File Tree, Conversation history; Center: Chat & Agent stream with collapsible `reasoning_content`, Writing Style & Reasoning Effort controls, Tool badges with Undo & Approval cards; Right: Artifacts & Local File Studio with Markdown Preview, Code/Text Editor, Diff viewer, and Terminal output).

**Tech Stack:** Tauri v2, Rust (`serde`, `serde_json`, `reqwest`, `tokio`, `futures-util`, `uuid`, `chrono`), React 19, TypeScript, Vite, Tailwind CSS, `lucide-react`, `marked` / custom Markdown renderer.

---

### Task 1: Project Scaffolding (Vite + React + TypeScript + Tauri v2)

**Files:**
- Create: `c:\DOCS\EXAONE\package.json`
- Create: `c:\DOCS\EXAONE\tsconfig.json`
- Create: `c:\DOCS\EXAONE\vite.config.ts`
- Create: `c:\DOCS\EXAONE\index.html`
- Create: `c:\DOCS\EXAONE\src-tauri\Cargo.toml`
- Create: `c:\DOCS\EXAONE\src-tauri\build.rs`
- Create: `c:\DOCS\EXAONE\src-tauri\tauri.conf.json`
- Create: `c:\DOCS\EXAONE\src-tauri\capabilities\default.json`

- [ ] **Step 1: Initialize frontend and Tauri configuration files**
- [ ] **Step 2: Verify `npm install` and `cargo check` succeed**

### Task 2: Rust Backend — Local Workspace File System & Snapshot Manager (`fs_ops.rs` & `store.rs`)

**Files:**
- Create: `c:\DOCS\EXAONE\src-tauri\src\store.rs`
- Create: `c:\DOCS\EXAONE\src-tauri\src\fs_ops.rs`

- [ ] **Step 1: Write unit tests in `fs_ops.rs` for `read_file`, `write_file`, `modify_file`, `delete_path`, `list_directory`, and snapshot restore (`undo_snapshot`)**
- [ ] **Step 2: Run `cargo test` to verify tests pass**
- [ ] **Step 3: Implement `store.rs` for saving/loading settings (API key, `reasoning_effort`, writing style), projects (local path, instructions), conversations, and file snapshots**

### Task 3: Rust Backend — K-EXAONE 2.5 API Client & Autonomous Agent Loop (`exaone.rs` & `lib.rs`)

**Files:**
- Create: `c:\DOCS\EXAONE\src-tauri\src\exaone.rs`
- Create: `c:\DOCS\EXAONE\src-tauri\src\lib.rs`
- Create: `c:\DOCS\EXAONE\src-tauri\src\main.rs`

- [ ] **Step 1: Implement K-EXAONE 2.5 request/response structures, SSE streaming parser (`reasoning_content`, `content`, `tool_calls`), and tool definitions (`list_directory`, `read_file`, `write_file`, `modify_file`, `search_files`, `delete_path`, `run_command`)**
- [ ] **Step 2: Implement autonomous execution for safe/edit tools (`list_directory`, `read_file`, `write_file`, `modify_file`, `search_files`) with automatic snapshot creation, and pause-for-approval state for `delete_path` and `run_command`**
- [ ] **Step 3: Run `cargo test` and `cargo check` to verify Rust backend compiles cleanly**

### Task 4: Frontend — Claude-Style 3-Pane Workspace, Project File Tree, Chat & Reasoning Stream, and Artifacts/Diff Studio

**Files:**
- Create: `c:\DOCS\EXAONE\src\types.ts`
- Create: `c:\DOCS\EXAONE\src\api.ts`
- Create: `c:\DOCS\EXAONE\src\App.tsx`
- Create: `c:\DOCS\EXAONE\src\main.tsx`
- Create: `c:\DOCS\EXAONE\src\styles.css`

- [ ] **Step 1: Create TypeScript interfaces and Tauri IPC bridge (with graceful web fallback for browser preview)**
- [ ] **Step 2: Implement Left Sidebar (Project manager connected to real local directories, Project system instructions, Live File Tree with Read/Create/Delete, Conversation list)**
- [ ] **Step 3: Implement Center Chat & Agent Pane (Streaming `reasoning_content` accordion, Markdown rendering, Writing style templates & `reasoning_effort` selector, Tool execution cards with Undo Snapshot button & Delete/Terminal Approval buttons)**
- [ ] **Step 4: Implement Right Artifacts & File Studio Pane (Markdown/HTML Preview, Live File Editor with Save to disk, Unified Diff Viewer comparing before/after snapshots, and Terminal output log)**

### Task 5: Build & End-to-End Verification

- [ ] **Step 1: Run `cargo test` in `src-tauri` and `npm run build` in root**
- [ ] **Step 2: Launch the application and verify UI & local file operations**
