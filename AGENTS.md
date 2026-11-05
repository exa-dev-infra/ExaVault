# Antigravity Agent Guidelines & Task Management Rules

Dokumen ini berisi pedoman permanen untuk AI Coding Assistant saat berinteraksi dan mengelola repository **ExaVault**.

---

## 📌 Aturan Task Management (MANDATORY)

Setiap agen yang bekerja pada repository ini **WAJIB** mematuhi siklus task management berikut secara mandiri tanpa perlu diingatkan oleh pengguna:

1. **Target Project ID**:
   - Target Project ID resmi untuk repository **ExaVault** di sistem Task Management adalah **Project ID: 13** (`name: ExaVault`, `color: #8b5cf6`).
   - Setiap pembuatan task baru (`create_task`), **WAJIB menyertakan `project_id: 13`** serta tags `["exavault", "backend", "security"]`.

2. **Pencarian & Pembuatan Task Aktif**:
   - Setiap kali user meminta penambahan fitur, perbaikan bug, atau enhancement baru: **selalu cari task aktif terlebih dahulu** menggunakan tool `list_tasks` (filter `project_id: 13`, status `in_progress` / `todo` atau keyword pencarian).
   - Jika task untuk pekerjaan tersebut belum ada, **otomatis buatkan task baru** (`create_task`) dengan status `in_progress`, cantumkan deskripsi jelas, dan pecah menjadi checklist `subtasks`.

3. **Penyelesaian & Logging Progres**:
   - Setelah pekerjaan selesai diimplementasikan, diverifikasi (build/test lulus), dan siap di-commit/push, **otomatis perbarui progress dan status task**:
     - Gunakan `log_task_progress` untuk mencatat catatan penyelesaian.
     - Update subtask yang selesai dan ubah status task utama menjadi `done` (`update_task_status`).

4. **Eksekusi Mandiri**:
   - Jalankan seluruh siklus task management ini (Cari -> Buat jika belum ada -> Kerjakan -> Verifikasi -> Mark Done) secara otonom di setiap giliran kerja tanpa perlu konfirmasi manual dari user untuk sekadar update task.

---

## 🏗️ Standar Arsitektur & Teknologi Repo

- **Tech Stack**: 100% Pure Rust (Axum, Tokio, SQLx PostgreSQL, Askama Template Engine, AES-256-GCM, SHA-256, Argon2/JWT).
- **Frontend / UI**: Embedded Server-Side Rendering via Askama + Modern Dark Mode Tailwind CSS + Alpine.js / Vanilla JS untuk interaktivitas (Key-Value Table, Raw `.env` bulk mode, mask/unmask, modal tokens).
- **Database**: PostgreSQL (port 54321 / `DATABASE_URL=postgres://postgres:postgres@localhost:54321/exavault`) dengan cascading deletes (`ON DELETE CASCADE`).
- **Security & Kriptografi**:
  - Enkripsi at rest: `AES-256-GCM` dengan 32-byte master key (`EXAVAULT_MASTER_KEY`) dan 12-byte IV unik per operasi write/update.
  - Service Token: Format `exv_<32_random_hex>`, disimpan sebagai hash `SHA-256` di database dan ditampilkan mentah hanya 1 kali saat pembuatan.
- **Portabilitas & AI Integration**:
  - Built-in MCP Server: sub-command `exavault mcp` (JSON-RPC stdio) untuk integrasi langsung ke IDE AI (Antigravity, Cursor, Claude Code).
  - CLI pull: `curl` export endpoint (`/api/v1/export`) dan `exavault pull`.

