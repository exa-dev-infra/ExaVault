# 🦀 Panduan Kilat & Shortcut Belajar Rust (Codebase Reference Guide)

Dokumen ini dirancang sebagai **cheat sheet dan jalan pintas (shortcut)** komprehensif bagi Anda yang ingin memahami sintaks, konsep, dan arsitektur Rust yang digunakan pada template `template-rust` dan `template-worker-rust`.

---

## 📑 Daftar Isi
1. [Mental Model Rust: Ownership, Borrowing & Lifetimes](#1-mental-model-rust)
2. [Bedah Sintaks & Kata Kunci (Keywords)](#2-bedah-sintaks--kata-kunci)
3. [Smart Pointers: Kapan Pakai Arc, Box, Mutex?](#3-smart-pointers)
4. [Penanganan Error: Result, Option, dan Operator ?](#4-penanganan-error-tanpa-trycatch)
5. [Atribut `#[...]` & Macro `!...`](#5-atribut-derive--macro)
6. [Bedah Komponen Stack: Axum, Tokio, SQLx, Tracing](#6-bedah-komponen-stack)
7. [Studi Kasus Baris-per-Baris dari Template Ini](#7-studi-kasus-kode-template)
8. [Tabel Komparasi Sintaks (Rust vs Go vs TypeScript vs Python)](#8-tabel-komparasi-sintaks)

---

## 1. Mental Model Rust

Di bahasa lain (JS/Python/Go/Java), memori dibersihkan oleh **Garbage Collector** di runtime atau dialokasikan manual (C/C++). Rust memilih jalur ketiga: **Compile-Time Ownership**.

### A. Ownership (Kepemilikan)
Setiap nilai di Rust memiliki **satu pemilik (owner)** pada satu waktu:
```rust
let s1 = String::from("halo");
let s2 = s1; // Ownership berpindah (MOVE) ke s2!
// println!("{}", s1); // ❌ ERROR: s1 sudah tidak valid karena kepemilikan berpindah
```

### B. Borrowing (Meminjam Referensi `&`)
Jika Anda hanya ingin membaca atau meminjam tanpa memindahkan kepemilikan, gunakan tanda `&`:
- **Immutable Borrow (`&T`)**: Meminjam hanya untuk membaca. Boleh ada banyak pembaca sekaligus.
- **Mutable Borrow (`&mut T`)**: Meminjam untuk mengubah. **Hanya boleh ada 1 peminjam mutable dalam satu waktu**, dan tidak boleh ada pembaca bersamaan. Ini menjamin **zero data races** di level compiler!

```rust
fn print_user(name: &String) { // Pinjam (baca saja)
    println!("{}", name);
}

fn update_name(name: &mut String) { // Pinjam untuk diubah
    name.push_str(" Developer");
}
```

### C. Lifetimes (`'a`, `'_`, `'static`)
Tanda petik satu `'` adalah **Lifetime Annotation**. Ini cara kita memberi tahu compiler: *"Berapa lama referensi ini dijamin tetap hidup di memori"*.
- `'a`: Nama generic lifetime.
- `'_`: *Anonymous / Elided lifetime* — kita meminta compiler menebak sendiri masa hidup referensinya (contoh: `&mut sqlx::Transaction<'_, sqlx::Postgres>`).
- `'static`: Nilai ini hidup selama seluruh program berjalan (contoh: string literal `"halo"` bertipe `&'static str`).

---

## 2. Bedah Sintaks & Kata Kunci

### `pub` & `pub(crate)` (Visibility)
Di Rust, **semua fungsi, struct, field, dan modul bersifat PRIVATE secara default**:
- `pub`: Terbuka untuk umum (bisa diakses modul lain di luar atau crate lain).
- `pub(crate)`: Hanya bisa diakses di dalam crate/proyek yang sama, tidak terekspos ke luar.
- `pub mod auth;`: Mengekspos folder/file modul `auth`.

### `fn` vs `async fn`
- `fn calculate()`: Fungsi sinkron standar.
- `async fn get_user()`: Fungsi asinkron yang mengembalikan tipe `Future`. Di balik layar, Rust tidak langsung mengeksekusi body-nya sampai fungsi tersebut dipanggil dengan `.await`.

### `struct` vs `enum`
- **`struct`**: Tipe data rekaman dengan field bernama (seperti objek/kelas DTO):
  ```rust
  pub struct UserResponse {
      pub id: Uuid,
      pub name: String,
      pub email: String,
  }
  ```
- **`enum` (Algebraic Data Types)**: Jauh lebih canggih daripada enum di Java/TS. Setiap varian di Rust bisa membawa data/payload:
  ```rust
  pub enum AppError {
      NotFound(String, String), // Membawa (message, code)
      Validation { message: String, details: Option<Value> }, // Membawa struct fields
      Database(sqlx::Error),   // Membungkus error lain
  }
  ```

### `impl` dan `trait`
- **`trait`**: Mirip dengan `interface` di TypeScript atau Go. Mendefinisikan kontrak method:
  ```rust
  #[async_trait]
  pub trait UserRepository: Send + Sync {
      async fn find_by_id(&self, pool: &PgPool, id: Uuid) -> Result<Option<UserResponse>, AppError>;
  }
  ```
- **`impl Trait for Struct`**: Mengimplementasikan kontrak tersebut:
  ```rust
  impl UserRepository for PostgresUserRepository {
      async fn find_by_id(&self, pool: &PgPool, id: Uuid) -> Result<Option<UserResponse>, AppError> {
          // Isi query database
      }
  }
  ```
- **`dyn Trait` (Dynamic Dispatch)**: Digunakan saat kita ingin polymorphism di runtime (contoh: `Arc<dyn UserRepository>`), mirip pointer interface di Go.

### `match` & `if let`
Pattern matching wajib mencakup seluruh kemungkinan (*exhaustive*):
```rust
match result {
    Ok(user) => println!("User: {}", user.name),
    Err(AppError::NotFound(msg, code)) => println!("Not found: {}", msg),
    Err(e) => println!("Other error: {}", e),
}

// Shortcut jika hanya peduli pada satu kondisi:
if let Some(token) = body.refresh_token {
    service.logout(&token).await?;
}
```

---

## 3. Smart Pointers

Kapan harus membungkus nilai dengan `Arc`, `Box`, atau `Mutex`?

| Smart Pointer | Kapan Digunakan? | Contoh di Template Ini |
|---|---|---|
| **`Box<T>`** | Mengalokasikan data ke **Heap** dengan ukuran pointer tetap di Stack. Sering dipakai untuk polymorphic errors. | `Result<(), Box<dyn std::error::Error>>` di `main.rs`. |
| **`Arc<T>`** | **A**tomic **R**eference **C**ounted pointer. Dipakai untuk **berbagi data bacaan yang sama antar banyak async thread** dengan thread-safe. | `Arc<UserService>`, `Arc<RealtimeHub>` yang diinjeksi ke Axum Router State. |
| **`tokio::sync::Mutex<T>`** | Jika data di dalam `Arc` butuh diubah (**mutasi**) oleh banyak async task secara bergantian. | Antrian state atau koneksi bersama. |

> **Aturan Praktis**: Jika sebuah Service atau Shared Hub ingin dibagikan ke banyak handler HTTP di Axum, bungkus selalu dengan `Arc::new(service)`.

---

## 4. Penanganan Error (Tanpa `try/catch`!)

Rust sama sekali tidak memiliki `try`, `catch`, `throw`, atau `null`.

### `Option<T>` (Nilai Ada atau Tidak)
Pengganti `null`/`undefined`:
- `Some(value)`: Nilai ada.
- `None`: Nilai kosong/tidak ada.
```rust
let maybe_user: Option<UserResponse> = repo.find_by_id(...).await?;
match maybe_user {
    Some(u) => Ok(u),
    None => Err(AppError::user_not_found()),
}
```

### `Result<T, E>` (Operasi Berhasil atau Gagal)
- `Ok(T)`: Berisi hasil sukses bertipe `T`.
- `Err(E)`: Berisi alasan kegagalan bertipe `E`.

### Operator Tanda Tanya `?`
Operator `?` adalah shortcut paling disukai di Rust:
```rust
// Tanpa ?:
let pool = match create_postgres_pool(&url).await {
    Ok(p) => p,
    Err(e) => return Err(e), // Langsung return keluar fungsi
};

// DENGAN OPERATOR ?:
let pool = create_postgres_pool(&url).await?; // Sangat ringkas!
```
Jika operasi bernilai `Ok(val)`, operator `?` langsung mengekstrak `val`. Jika `Err(err)`, operator `?` otomatis melakukan `return Err(...)` dari fungsi saat itu juga.

---

## 5. Atribut `#[...]` & Macro `!...`

### Atribut Derive `#[derive(...)]`
Di atas struct, Anda sering melihat:
```rust
#[derive(Debug, Serialize, Deserialize, Validate, FromRow, Clone)]
pub struct CreateUserRequest { ... }
```
Compiler otomatis men-generate kode boilerplate untuk struct tersebut:
- `Debug`: Mengizinkan dicetak via format `println!("{:?}", x)`.
- `Serialize` & `Deserialize`: Mengizinkan konversi otomatis ke/dari JSON via crate `serde`.
- `Validate`: Mengizinkan validasi deklaratif (panjang teks, format email) via crate `validator`.
- `FromRow`: Mengizinkan SQLx membaca baris query PostgreSQL langsung dipetakan ke struct ini.
- `Clone`: Mengizinkan objek diduplikasi via `.clone()`.

### Macro dengan Tanda Seru `!`
Macro di Rust dijalankan saat proses kompilasi untuk menulis kode:
- `json!({ "success": true, "count": 10 })`: Membuat objek JSON literal.
- `format!("User: {}", id)`: String concatenation (mirip template string di TS `` `User: ${id}` ``).
- `vec![1, 2, 3]`: Membuat `Vec<T>` (array dinamis).
- `tracing::info!(user_id = %id, "Action logged")`: Menulis log terstruktur JSON.

---

## 6. Bedah Komponen Stack

### 🚀 Axum (Web Framework)
- **Router**: Pemetaan URL ke fungsi handler (`Router::new().route("/profile", get(get_profile))`).
- **Extractor Pattern**: Parameter di fungsi handler Axum diekstrak otomatis dari HTTP request berdasarkan tipenya:
  - `State(svc)`: Mengambil dependensi service yang didaftarkan di root router.
  - `ValidatedJson(body)`: Mengekstrak body JSON sekaligus memvalidasi aturannya.
  - `Query(q)`: Mengekstrak query parameters (`?page=1&limit=10`).
  - `AuthenticatedUser`: Custom extractor yang otomatis memeriksa header `Authorization: Bearer <token>` dan menolak request dengan 401 jika token invalid.

### ⚡ Tokio (Async Runtime)
Rust tidak punya built-in event loop seperti Node.js. Tokio adalah runtime async performa tinggi untuk Rust:
- `#[tokio::main]`: Mengaktifkan multithreaded async runtime di fungsi `main()`.
- `tokio::select!`: Menunggu event pertama yang selesai (misal: sinyal `Ctrl+C` vs event baru di loop).
- `tokio::sync::broadcast`: Antrian channel pub/sub untuk WebSocket dan SSE realtime broadcast.

### 🗄️ SQLx (PostgreSQL Driver)
- **Pure Async**: Dibangun langsung di atas Tokio tanpa memblokir thread.
- **Connection Pool (`PgPool`)**: Mengelola banyak koneksi secara efisien.
- **Pure Transaction Passing**:
  ```rust
  // Mulai transaksi di layer Service:
  let mut tx = self.pool.begin().await.map_err(AppError::Database)?;
  // Berikan referensi mutable transaksi ke Repository:
  let user = self.user_repo.create(&mut tx, new_user).await?;
  // Commit jika semua lancar:
  tx.commit().await.map_err(AppError::Database)?;
  ```
  Jika terjadi error sebelum `tx.commit()`, objek `tx` otomatis di-drop dan PostgreSQL otomatis melakukan **ROLLBACK** tanpa kebocoran transaksi!

### 📝 Tracing (Structured JSON Logging)
Aturan monorepo melarang raw `println!`. Sebagai gantinya digunakan `tracing`:
```rust
tracing::info!(
    user_id = %user.id,
    email = %user.email,
    "New user registered successfully"
);
```
Output log otomatis berupa satu baris JSON valid:
```json
{"timestamp":"2026-11-03T03:00:00Z","level":"INFO","fields":{"user_id":"...","email":"...","message":"New user registered successfully"}}
```

---

## 7. Studi Kasus Kode Template

### Kasus 1: Mengapa Handler di `user/handler.rs` Sangat Bersih?
Lihat fungsi ini di [`src/modules/user/handler.rs`](../src/modules/user/handler.rs):

```rust
pub async fn update_profile_handler(
    auth_user: AuthenticatedUser,                         // 1. Cek Token JWT & Autentikasi
    State(service): State<Arc<UserService>>,              // 2. Ambil Service Dependency
    ValidatedJson(req): ValidatedJson<UpdateUserRequest>, // 3. Ambil & Validasi Body JSON
) -> Result<impl IntoResponse, AppError> {                // 4. Return Response atau AppError
    let updated = service.update_profile(auth_user.id, req).await?;
    Ok(BaseResponse::success(updated, "Profile updated successfully"))
}
```
**Penjelasan Ajaibnya:**
1. Jika token JWT tidak ada / expired, Axum langsung menolak request dengan status **401 Unauthorized** sebelum fungsi handler ini sempat dipanggil.
2. Jika JSON tidak valid (misal nama kosong), `ValidatedJson` langsung menolak dengan status **422 Unprocessable Entity**.
3. Di dalam handler, kita hanya menulis **2 baris kode bisnis** murni!

---

### Kasus 2: Custom Extractor di `pkg/auth/jwt.rs`
Bagaimana cara `AuthenticatedUser` memeriksa header token?

```rust
#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Ambil header Authorization
        let auth_header = parts.headers.get("Authorization")...;
        // Verifikasi JWT token
        let claims = verify_access_token(token, &secret)?;
        // Jika valid, teruskan data user ke handler:
        Ok(AuthenticatedUser { id: claims.sub, email: claims.email, role: claims.role })
    }
}
```

---

### Kasus 3: SHA-256 Refresh Token Hashing & Reuse Detection
Di [`src/modules/auth/service.rs`](../src/modules/auth/service.rs):
1. **Hashing SHA-256**: Raw refresh token tidak pernah disimpan mentah di DB. Sebelum disimpan atau dicari, selalu di-hash:
   ```rust
   let token_digest = hash_token(raw_refresh_token);
   ```
2. **Reuse Detection (Pertahanan Pencurian Token)**:
   ```rust
   if token_record.revoked_at.is_some() {
       // Seseorang mencoba memakai token yang sudah dicabut!
       // Kemungkinan token dicuri oleh peretas.
       // CABUT SELURUH SESI USER SEKETIKA:
       self.auth_repo.revoke_all_user_tokens(&mut tx, token_record.user_id).await?;
       return Err(AppError::unauthorized("Suspicious activity, token reused", ...));
   }
   ```

---

### Kasus 4: OpenTelemetry Integration dengan Tracing
Di [`src/pkg/logging/mod.rs`](../src/pkg/logging/mod.rs) dan [`src/pkg/telemetry/mod.rs`](../src/pkg/telemetry/mod.rs):
- Menggabungkan layer `tracing_subscriber::fmt::layer().json()` (log JSON ke stdout) dengan `tracing_opentelemetry::layer().with_tracer(tracer)` (distributed trace ke Jaeger / OTLP Collector).
- Otomatis meng-attach context propagation W3C `traceparent` sehingga span request terhubung dari frontend hingga backend microservices.

---

### Kasus 5: Distributed Realtime Bridge (Redis Pub/Sub & RabbitMQ Fanout)
Di [`src/pkg/realtime/bridge.rs`](../src/pkg/realtime/bridge.rs) dan [`src/pkg/realtime/hub.rs`](../src/pkg/realtime/hub.rs):
Ketika backend di-scale horizontal menjadi banyak replika (pod/container), WebSocket client yang tersambung di Pod A tidak akan menerima pesan dari Pod B tanpa broker distributor:
1. **Trait Polimorfisme (`RealtimeBridge`)**:
   ```rust
   #[async_trait]
   pub trait RealtimeBridge: Send + Sync {
       fn name(&self) -> &str;
       async fn publish(&self, channel: &str, message: &str) -> Result<(), ...>;
   }
   ```
2. **Background Listening Task**:
   Setiap instance aplikasi menjalankan background `tokio::spawn` yang subscribe ke Redis channel `realtime:broadcast` atau RabbitMQ fanout exchange `realtime.events`.
3. **Graceful Fallback**:
   Jika Redis atau RabbitMQ tidak tersedia atau gagal terkoneksi, aplikasi otomatis mendegradasi ke `LocalBridge` (in-memory tokio broadcast channel) tanpa crash atau panik:
   ```rust
   warn!(error = %err, "Failed to connect Redis realtime bridge. Falling back to Local in-memory mode.");
   Arc::new(LocalBridge)
   ```

---

## 8. Tabel Komparasi Sintaks

| Konsep | Rust | TypeScript | Go | Python |
|---|---|---|---|---|
| **Deklarasi Variabel Tetap** | `let x = 5;` | `const x = 5;` | `x := 5` | `x = 5` |
| **Deklarasi Variabel Ubah** | `let mut x = 5;` | `let x = 5;` | `var x = 5` | `x = 5` |
| **Pengecekan Nullable** | `Option<T>` (`Some` / `None`) | `T \| null \| undefined` | `*T` (`nil`) | `Optional[T]` (`None`) |
| **Return Error** | `Result<T, E>` | `throw new Error()` | `(T, error)` | `raise Exception()` |
| **Propagasi Error Singkat** | `let data = fetch()?;` | *Tidak ada* (harus `try/catch`) | `if err != nil { return err }` | *Tidak ada* |
| **Async Call** | `do_something().await` | `await doSomething()` | *Goroutine channel* | `await do_something()` |
| **Public / Export** | `pub fn do_it()` | `export function doIt()` | `func DoIt()` (Huruf kapital) | `def do_it()` (Default public) |
| **Konversi JSON** | `serde_json::to_string(&data)` | `JSON.stringify(data)` | `json.Marshal(data)` | `json.dumps(data)` |

---

## 💡 Tips & Trik Pengembangan di Rust

1. **Jadikan Compiler Teman Terbaik Anda**: Pesan error compiler Rust (`rustc`) adalah yang paling jelas dan edukatif di industri. Selalu baca teks `help:` yang diberikan compiler.
2. **Jalankan `cargo clippy` Rutin**: Linter Clippy akan memberi tahu Anda cara menulis kode yang lebih idiomatis dan cepat.
3. **Gunakan `make check`**: Selalu pastikan `make check` hijau sebelum menganggap pekerjaan selesai.

