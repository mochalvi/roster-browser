<div align="center">
  <img src="branding/logo.png" alt="Roster Browser logo" width="120" />
  <h1>Roster Browser</h1>
  <p><strong>Manage hundreds of Google accounts by account, not by profile.</strong></p>
  <p><em>Kelola ratusan akun Google per akun, bukan per profil.</em></p>
</div>

<div align="center">
  <img src="docs/screenshot.jpg" alt="Roster Browser — account dashboard (demo data)" width="820" />
  <p><sub>Demo data — real accounts are not shown. · Data contoh — bukan akun asli.</sub></p>
  <p>
    <a href="https://github.com/mochalvi/roster-browser/releases/latest"><strong>⬇️ Download for Windows</strong></a>
  </p>
</div>

---

> **English below · [Bahasa Indonesia di bawah](#bahasa-indonesia)**

## English

### Why Roster

Managing many Google accounts in anti-detect browsers is slow: you open a
profile, wait for it to load, then check which account is inside — one profile at
a time. Roster flips this around. It lists your **accounts** directly as cards.
Click an account and it opens straight into Google signed in as that account. No
hunting through profiles first.

Under the hood Roster still uses browser profiles (it is a fork of
[Donut Browser](https://github.com/zhom/donutbrowser)), but it treats a profile
as a *slot holder*: Google allows up to 10 accounts signed in at once per
browser, so each profile holds up to 10 accounts and Roster opens a new hidden
profile when the slots fill. You see accounts; the profiles stay out of your way.

### Features

- **Account-first dashboard** — every Google account as a searchable card with
  avatar, status, tags, and last-refresh time.
- **One-click open** — opens `google.com` already signed in as the chosen
  account (via `authuser`).
- **Hidden profile slots** — up to 10 accounts per profile, auto-allocated, with
  an option to group related accounts into the same profile.
- **Sync / refresh** — keeps sessions warm in the background and shows a live
  per-account result (⏳ queued → 🔄 running → ✅ done · ⚠️ needs verification ·
  ❌ logged out).
- **Per-profile proxy & consistent fingerprint** — inherited from Donut's
  anti-detect engine; each account keeps a stable proxy, timezone, and
  fingerprint.
- **Google AI plan badge** — every Sync reads each account's plan (Pro, Ultra,
  …) from Google's own account menu, the same badge Google shows under your
  avatar. Accounts without a plan show **Free**.
- **Antigravity status + appeal** — mark an account as *Antigravity OK*,
  *banned*, or *appeal sent*. A banned account gets an **Appeal** button that
  opens Google's official suspension appeal form in that account's own profile,
  already signed in. Google allows only one appeal per account.
- **Encrypted export / import (`.roster`)** — copy your accounts and their
  sessions to another computer as one password-protected file (AES-256-GCM,
  Argon2id key derivation). The password is verified and the file decrypted
  whole *before* anything is written, so a wrong password changes nothing.
  Import is additive — it brings accounts in alongside what is already there.

### Keeping sessions alive — what actually works

There is no trick that guarantees a Google session survives forever; Google can
ask for verification at any time. What Roster does to maximise session lifetime:

- **After moving, stop using the account on the old machine.** The same session
  running in two places is the number-one cause of forced logout. Export copies
  rather than locks, so this discipline is on you until an opt-in "move" mode
  lands.
- **The proxy travels with the account** inside the profile, so the account
  keeps the same exit IP after import.
- **Fingerprint and timezone stay constant** per account.
- **Sessions are portable by design.** Roster's engine (Wayfern) keeps each
  profile's cookie-encryption key *inside the profile folder*, not in the OS
  keychain, so an exported session can still be decrypted on another machine.

When an account still lands on ⚠️, the safe fallback is a manual re-login in the
same profile — because the proxy and fingerprint are identical, Google usually
does not challenge it hard.

### Why there is no "import from Chrome / Edge"

Roster used to offer importing a profile from another browser. It was removed:
Chrome, Edge, and Brave on Windows seal their cookies with App-Bound Encryption,
which only the original browser can open, so Google sign-ins never came along.
Sign in to each account inside its Roster profile instead — a session created
there keeps a consistent fingerprint and proxy from the start, which is also
what makes it last.

### Why Antigravity status is set by hand

Google reports an Antigravity suspension only through Antigravity's own API,
which needs Antigravity's sign-in token. Using that token from a third-party
tool is itself a widely reported ban trigger, so Roster never checks it for
you. Set the status yourself when Antigravity shows the "disabled … violation
of Terms of Service" error.

### Status

This is an active fork in development. The account layer, launcher, refresh, and
export/import are being built on top of Donut's profile engine.

### Download

Pre-built installers are on the
**[Releases page](https://github.com/mochalvi/roster-browser/releases/latest)**:

| Platform | File |
|---|---|
| Windows 10/11 (x64) | `Roster_<version>_x64-setup.exe` |
| macOS — **Apple Silicon only** (M1–M4) | `Roster_<version>_aarch64.dmg` |
| Linux (x64) | `.AppImage`, `.deb`, or `.rpm` |

> [!IMPORTANT]
> **The macOS build runs only on Apple Silicon Macs (M1, M2, M3, M4). It does
> not run on Intel Macs** — macOS shows *"You can't open the application Roster
> because this application is not supported on this Mac."*
> To check your Mac: **Apple menu → About This Mac**. *Chip: Apple M…* means
> Apple Silicon (supported); *Processor: Intel…* means Intel (not supported).
> Intel Macs and ARM Linux cannot be supported because Roster's browser engine
> (Wayfern) is not published for them.

Step-by-step install instructions for every platform are on the
[release page](https://github.com/mochalvi/roster-browser/releases/latest).

The builds are not code-signed yet:

- **Windows:** SmartScreen may warn — choose *More info → Run anyway*.
- **macOS:** after moving Roster to Applications, run this once in Terminal,
  otherwise macOS reports the app as damaged:
  `xattr -cr /Applications/Roster.app`

Roster does not update itself; download new versions from the Releases page.

### Build

Roster is a [Tauri](https://tauri.app) app (Rust + Next.js). Requirements:

- Node.js 20+ and `pnpm`
- Rust (stable) and the platform C/C++ build tools

```bash
pnpm install
pnpm tauri dev      # run
pnpm tauri build    # package
```

### License

Roster Browser is licensed under the **GNU AGPL-3.0**, inherited from Donut
Browser. The full text is in [LICENSE](LICENSE). If you distribute a modified
version, including over a network, you must make your source available under the
same license.

Roster is a fork of **[Donut Browser](https://github.com/zhom/donutbrowser)** by
zhom. Credit for the profile, proxy, and anti-detect engine goes to the Donut
Browser project.

### Disclaimer

Roster Browser is a tool for managing **your own** Google accounts, or accounts
you are authorised to manage (for example a client's). You are responsible for
complying with Google's Terms of Service. Do not use it for mass account
creation, spam, fake engagement, or evading bans. The project ships no
account-creation automation and no CAPTCHA solving.

---

## Bahasa Indonesia

### Kenapa Roster

Mengelola banyak akun Google di anti-detect browser itu lambat: kamu buka
profil, tunggu loading, baru cek akun di dalamnya — satu profil satu per satu.
Roster membalik alur ini. Roster menampilkan **akun** langsung sebagai kartu.
Klik sebuah akun, browser langsung terbuka dalam keadaan login sebagai akun itu.
Tidak perlu buka-buka profil dulu.

Di belakang layar Roster tetap memakai profil browser (ini fork dari
[Donut Browser](https://github.com/zhom/donutbrowser)), tapi profil diperlakukan
sebagai *wadah slot*: Google mengizinkan maksimal 10 akun login bersamaan di satu
browser, jadi tiap profil menampung hingga 10 akun dan Roster otomatis membuat
profil tersembunyi baru saat slotnya penuh. Yang kamu lihat akun; profilnya
disembunyikan.

### Fitur

- **Dashboard berbasis akun** — tiap akun Google jadi kartu yang bisa dicari,
  lengkap dengan avatar, status, tag, dan waktu refresh terakhir.
- **Buka sekali klik** — langsung membuka `google.com` dalam keadaan login
  sebagai akun yang dipilih (lewat `authuser`).
- **Slot profil tersembunyi** — hingga 10 akun per profil, dialokasikan otomatis,
  dengan opsi mengelompokkan akun terkait ke profil yang sama.
- **Sync / refresh** — menjaga sesi tetap hidup di background dan menampilkan
  hasil per akun secara langsung (⏳ antri → 🔄 proses → ✅ selesai ·
  ⚠️ perlu verifikasi · ❌ logout).
- **Proxy per profil & fingerprint konsisten** — diwarisi dari mesin anti-detect
  Donut; tiap akun punya proxy, timezone, dan fingerprint yang tetap.
- **Badge paket Google AI** — setiap Sync membaca paket tiap akun (Pro, Ultra,
  …) dari menu akun Google sendiri, badge yang sama dengan yang Google tampilkan
  di bawah foto profilmu. Akun tanpa paket tampil **Free**.
- **Status Antigravity + appeal** — tandai akun sebagai *Antigravity OK*,
  *banned*, atau *appeal terkirim*. Akun yang banned mendapat tombol **Appeal**
  yang membuka form appeal resmi Google di profil akun itu sendiri, sudah dalam
  keadaan login. Google hanya mengizinkan satu kali appeal per akun.
- **Export / import terenkripsi (`.roster`)** — salin akun dan sesinya ke
  komputer lain sebagai satu file berpassword (AES-256-GCM, derivasi kunci
  Argon2id). Password diverifikasi dan file didekripsi utuh *sebelum* ada yang
  ditulis, jadi password salah tidak mengubah apa pun. Import bersifat aditif —
  membawa akun masuk berdampingan dengan yang sudah ada.

### Menjaga sesi tetap hidup — yang benar-benar berpengaruh

Tidak ada trik yang menjamin sesi Google awet selamanya; Google bisa meminta
verifikasi kapan pun. Yang Roster lakukan untuk memperpanjang umur sesi:

- **Setelah pindah, berhenti memakai akun itu di komputer lama.** Sesi yang sama
  jalan di dua tempat adalah penyebab utama logout paksa. Export menyalin, bukan
  mengunci, jadi disiplin ini ada di tanganmu sampai mode "pindah" opsional
  tersedia.
- **Proxy ikut bersama akun** di dalam profil, jadi akun tetap memakai IP keluar
  yang sama setelah import.
- **Fingerprint dan timezone tetap** untuk tiap akun.
- **Akun di-refresh tepat sebelum export** supaya sesi yang dipindah masih baru.
- **Sesi memang dirancang portabel.** Engine Roster (Wayfern) menyimpan kunci
  enkripsi cookie tiap profil *di dalam folder profilnya*, bukan di keychain OS,
  sehingga sesi yang diekspor tetap bisa dibuka di komputer lain.

Kalau sebuah akun tetap berakhir ⚠️, jalan amannya adalah login ulang manual di
profil yang sama — karena proxy dan fingerprint-nya identik, Google biasanya
tidak meminta verifikasi berat.

### Kenapa tidak ada "import dari Chrome / Edge"

Roster dulu punya fitur import profil dari browser lain. Fitur itu dihapus:
Chrome, Edge, dan Brave di Windows mengunci cookie dengan App-Bound Encryption
yang hanya bisa dibuka browser aslinya, jadi login Google tidak pernah ikut
terbawa. Login-lah ke tiap akun langsung di profil Roster-nya — sesi yang dibuat
di sana punya fingerprint dan proxy yang konsisten sejak awal, dan itulah yang
membuatnya awet.

### Kenapa status Antigravity diisi manual

Google hanya memberi tahu suspensi Antigravity lewat API Antigravity sendiri,
yang butuh token login Antigravity. Memakai token itu dari aplikasi pihak ketiga
justru banyak dilaporkan sebagai pemicu banned, jadi Roster tidak pernah
mengeceknya otomatis. Ubah statusnya sendiri saat Antigravity menampilkan error
"disabled … violation of Terms of Service".

### Status

Ini fork yang sedang aktif dikembangkan. Lapisan akun, launcher, refresh, dan
export/import sedang dibangun di atas mesin profil Donut.

### Unduh

Installer siap pakai ada di
**[halaman Releases](https://github.com/mochalvi/roster-browser/releases/latest)**:

| Platform | File |
|---|---|
| Windows 10/11 (x64) | `Roster_<versi>_x64-setup.exe` |
| macOS — **khusus Apple Silicon** (M1–M4) | `Roster_<versi>_aarch64.dmg` |
| Linux (x64) | `.AppImage`, `.deb`, atau `.rpm` |

> [!IMPORTANT]
> **Versi macOS hanya bisa berjalan di Mac Apple Silicon (M1, M2, M3, M4).
> Tidak bisa di Mac Intel** — macOS akan menampilkan *"You can't open the
> application Roster because this application is not supported on this Mac."*
> Cara mengecek Mac-mu: **menu Apple → About This Mac**. Tertulis *Chip: Apple
> M…* berarti Apple Silicon (didukung); *Processor: Intel…* berarti Intel (tidak
> didukung). Mac Intel dan Linux ARM tidak bisa didukung karena mesin browser
> Roster (Wayfern) tidak dirilis untuk platform itu.

Langkah instalasi lengkap untuk tiap platform ada di
[halaman rilis](https://github.com/mochalvi/roster-browser/releases/latest).

Build-nya belum ditandatangani (code-signing):

- **Windows:** SmartScreen mungkin memberi peringatan — pilih *More info → Run
  anyway*.
- **macOS:** setelah memindahkan Roster ke Applications, jalankan sekali perintah
  ini di Terminal, kalau tidak macOS akan menyebut aplikasinya rusak:
  `xattr -cr /Applications/Roster.app`

Roster tidak meng-update dirinya sendiri; unduh versi baru dari halaman Releases.

### Build

Roster adalah aplikasi [Tauri](https://tauri.app) (Rust + Next.js). Kebutuhan:

- Node.js 20+ dan `pnpm`
- Rust (stable) dan build tools C/C++ sesuai platform

```bash
pnpm install
pnpm tauri dev      # jalankan
pnpm tauri build    # paket rilis
```

### Lisensi

Roster Browser berlisensi **GNU AGPL-3.0**, diwarisi dari Donut Browser. Teks
lengkapnya ada di [LICENSE](LICENSE). Jika kamu mendistribusikan versi yang sudah
dimodifikasi, termasuk lewat jaringan, kamu wajib menyediakan source code-nya
dengan lisensi yang sama.

Roster adalah fork dari **[Donut Browser](https://github.com/zhom/donutbrowser)**
oleh zhom. Kredit untuk mesin profil, proxy, dan anti-detect ada pada proyek
Donut Browser.

### Penafian

Roster Browser adalah alat untuk mengelola akun Google **milikmu sendiri**, atau
akun yang kamu diberi wewenang mengelolanya (misalnya milik klien). Kamu
bertanggung jawab mematuhi Ketentuan Layanan Google. Jangan gunakan untuk
pembuatan akun massal, spam, engagement palsu, atau mengakali banned. Proyek ini
tidak menyertakan otomasi pembuatan akun maupun pemecah CAPTCHA.
