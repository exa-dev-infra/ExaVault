---
name: rust-backend
description: Architectural guidelines, Axum routing, SQLx pure parameter passing, structured tracing, and quality gates for the Rust backend template.
---

# Rust Backend Developer Skill (`template-rust`)

Gunakan skill ini saat membuat atau memodifikasi modul, endpoint, service, repository, maupun migrasi database pada template Rust (`template-rust`).

---

## 1. Quality Gates & Migration Commands

Setiap perubahan kode **WAJIB** memvalidasi perintah berikut:

```bash
# Quality gate utama
make check

# Komponen terpisah
cargo fmt --check
cargo clippy -- -D warnings
cargo test

# Manajemen Database Migration & Rollback
make migrate-up       # Jalankan migrasi baru
make migrate-down     # Rollback 1 migrasi terakhir
make migrate-status   # Cek status migrasi
make migrate-create name=nama_fitur # Buat file .up.sql dan .down.sql baru
```

---

## 2. Struktur Modul Baru

Saat menambahkan modul baru di `src/modules/<nama_modul>/`:

1. **DTO (`dto.rs`)**:
   Gunakan atribut `validator` untuk validasi deklaratif:
   ```rust
   use validator::Validate;
   use serde::Deserialize;

   #[derive(Debug, Deserialize, Validate)]
   pub struct CreateItemRequest {
       #[validate(length(min = 3, max = 100))]
       pub title: String,
   }
   ```

2. **Repository (`repository.rs`)**:
   Definisikan trait terlebih dahulu agar mudah di-mock pada unit testing:
   ```rust
   #[async_trait]
   pub trait ItemRepository: Send + Sync {
       async fn find_by_id(&self, pool: &PgPool, id: Uuid) -> Result<Option<ItemResponse>, AppError>;
       async fn create(&self, tx: &mut Transaction<'_, Postgres>, item: NewItem<'_>) -> Result<ItemResponse, AppError>;
   }
   ```

3. **Service (`service.rs`)**:
   Orkestrasi transaksi mutasi:
   ```rust
   pub async fn create_item(&self, req: CreateItemRequest) -> Result<ItemResponse, AppError> {
       let mut tx = self.pool.begin().await.map_err(AppError::Database)?;
       let item = self.repo.create(&mut tx, ...).await?;
       tx.commit().await.map_err(AppError::Database)?;
       Ok(item)
   }
   ```

4. **Handler (`handler.rs`)**:
   Gunakan `ValidatedJson` dan `BaseResponse`:
   ```rust
   pub async fn create_item_handler(
       State(service): State<Arc<ItemService>>,
       ValidatedJson(req): ValidatedJson<CreateItemRequest>,
   ) -> Result<impl IntoResponse, AppError> {
       let item = service.create_item(req).await?;
       Ok(BaseResponse::created(item, "Item created successfully"))
   }
   ```

