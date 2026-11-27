# Panduan Workflow Blender → TWG Rust & Bevy (Model 3D Animated & Aksi Tambahan)

Dokumen ini adalah panduan praktis menyiapkan karakter / model 3D beranimasi dari **Tripo3D → Mixamo → Blender → TWG-Rust (Bevy)**, termasuk cara menambahkan **Animasi Aksi Tambahan** (seperti *Melambai*, *Menembak*, *Pingsan*, *Hormat*, dll.) selain `Idle` dan `Walk`.

---

## 1. Perbedaan Pipeline Lama vs Pipeline Baru (Rust + Bevy)

Pada aplikasi lama (Web / MapTalks), prosesnya cukup panjang karena keterbatasan parser glTF browser. Pada **TWG Rust + Bevy**, alurnya jauh lebih singkat, bersih, dan langsung standar glTF 2.0:

| Tahap | Pipeline Lama (MapTalks) | Pipeline Baru (Rust + Bevy) |
| :--- | :--- | :--- |
| **Sumber Mesh & Rig** | Tripo3D → Mixamo (`.fbx`) | Tripo3D → Mixamo (`.fbx`) — **tetap sama** |
| **Rename Action** | Wajib rename `Idle\|Idle` & `Idle\|Walk` | **Tidak wajib** (cukup nama rapi, mis. `Walk`, `Melambai`, `Pingsan`) |
| **Export dari Blender** | Export `.fbx` → konversi via **Assimp** ke `.glb` | **Langsung Export `.glb` dari Blender!** (Tidak perlu Assimp) |
| **Kompresi Draco** | Wajib `gltf-transform draco` | **JANGAN pakai Draco!** Bevy membaca `.glb` non-Draco langsung; server TWG otomatis optimasi **LOD0 + LOD2** & normalisasi bobot tulang (`WEIGHTS_0`) |

> **Ringkasnya:** `Tripo3D` → `Mixamo (.fbx)` → `Blender` → **Export `.glb` langsung** → **Upload di TWG `Assets → 3D Models`**.

---

## 2. Alur Lengkap: Tripo3D → Mixamo → Blender

### Tahap A — Siapkan Karakter di Tripo3D & Mixamo
1. Export model karakter 3D dari **Tripo3D** (format `.fbx` atau `.glb` T-Pose/A-Pose).
2. Upload karakter ke **Mixamo** dan lakukan *Auto-Rigging* seperti biasa.
3. Download semua animasi yang dibutuhkan dari Mixamo dengan pengaturan:
   - **Format**: `FBX Binary (.fbx)`
   - **Skin**: **`With Skin`** *(wajib untuk setiap file agar tekstur, mesh, dan skeleton konsisten)*
   - **Frames per Second**: `30`
   - **Keyframe Reduction**: `none`
   - **In Place**: **CENTANG (`ON`)** khusus untuk animasi jalan (`Walk`) agar karakter berjalan di tempat (perpindahan posisi dikendalikan oleh sistem Rute TWG).
   - Untuk animasi aksi di tempat seperti *Melambai*, *Menembak*, *Pingsan*, *Hormat*: biarkan posisi karakter tetap di sekitar titik origin `(0, 0, 0)`.

Contoh hasil download dari Mixamo:
- `polisi_tpose.fbx` *(untuk dibuat file `Idle` tanpa animasi)*
- `polisi_walk.fbx` *(animasi jalan — In Place)*
- `polisi_melambai.fbx` *(aksi tambahan: mengatur lalu lintas)*
- `polisi_menembak.fbx` *(aksi tambahan: menembak — bisa di-Combine dengan Shoot)*
- `polisi_pingsan.fbx` *(aksi tambahan: berdiri → rebah ke tanah)*

---

### Tahap B — Menyiapkan 3 Jenis File di Blender

> **Tips Penting:** Mulai setiap file dari **scene Blender kosong** (`File → New → General`, lalu hapus Cube/Light/Camera bawaan), lalu `File → Import → FBX (.fbx)`.

#### 1. File `Idle` (`*_idle.glb`) — **WAJIB Tanpa Animasi**
File ini digunakan saat karakter diam/berdiri normal di peta.
1. `File → Import → FBX (.fbx)` → pilih pose diam / T-Pose / Standing Idle dari Mixamo.
2. Jika ada animasi/keyframe bawaan, pilih **Armature**, buka **Dope Sheet → Action Editor**, lalu klik tombol **`X`** (Unlink Action) agar tidak ada Action aktif — atau cukup pastikan opsi **Animation = OFF** saat export `.glb`.
3. Cek posisi & arah hadap karakter:
   - **Telapak kaki** tepat menyentuh lantai (`Z = 0` di Blender).
   - **Wajah / depan karakter** menghadap ke **`-Y` di Blender** (karena saat export glTF `+Y Up`, Blender `-Y` otomatis menjadi **`+Z` Forward** di Bevy, sesuai arah maju Rute TWG).
   - Cek **Apply Scale** jika perlu (`Ctrl + A → Scale`), tapi hati-hati jangan mematahkan rig Mixamo (umumnya FBX Mixamo yang langsung di-export `.glb` sudah otomatis benar skalanya).
4. Export sebagai `nama_model_idle.glb` dengan **Animation = OFF** (lihat tabel setting di Bagian 3).

#### 2. File `Walk` (`*_walk.glb`) — **WAJIB Memiliki Animasi Jalan**
File ini otomatis diputar oleh TWG ketika karakter bergerak di **Rute / Formasi / RTS**.
1. `File → Import → FBX (.fbx)` → pilih `polisi_walk.fbx` *(pastikan Mixamo **In Place = ON**)*.
2. Tekan `Spacebar` di viewport Blender untuk memastikan karakter berjalan di tempat dengan mulus (tidak meluncur maju menjauhi origin).
3. Pastikan arah hadap dan ukuran **sama persis** dengan file `Idle`.
4. Di **Action Editor**, opsional beri nama rapi misalnya `Walk`.
5. Export sebagai `nama_model_walk.glb` dengan **Animation = ON** (lihat tabel setting di Bagian 3).

#### 3. File `Aksi Tambahan` (`*_melambai.glb`, `*_menembak.glb`, `*_pingsan.glb`, dll.)
File ini digunakan untuk fitur **Aksi Model 3D** di TWG!
1. `File → Import → FBX (.fbx)` → pilih file aksi dari Mixamo (mis. `polisi_pingsan.fbx`).
2. Tekan `Spacebar` untuk cek animasinya:
   - **Untuk aksi berulang** (mis. *Melambai*): pastikan awal & akhir gerakan cukup mulus untuk di-loop.
   - **Untuk aksi tahan pose akhir** (mis. *Pingsan* dari berdiri ke rebah): pastikan frame terakhir berada tepat di pose rebah di lantai (jangan ada frame bangun lagi di akhir!).
   - **Untuk aksi sekali lalu kembali Idle** (mis. *Menembak*, *Hormat*): cukup biarkan panjang durasi aksinya natural — TWG otomatis mengembalikan model ke pose `Idle` setelah selesai.
3. Pastikan **tinggi, skala, dan arah hadap sama persis** dengan file `Idle` dan `Walk` agar saat transisi `Idle ↔ Aksi ↔ Walk` karakter tidak meloncat atau berubah ukuran.
4. Export sebagai `.glb` dengan **Animation = ON**.

---

## 3. Pengaturan Export glTF 2.0 (`.glb`) di Blender

Gunakan menu: **`File → Export → glTF 2.0 (.glb/.gltf)`**.

Berikut pengaturan yang **WAJIB** diterapkan agar model 100% muncul sempurna di **Rust + Bevy** (dan tidak pernah gagal seperti di pipeline lama):

### Tab Umum
| Pengaturan | Nilai | Keterangan |
| :--- | :--- | :--- |
| **Format** | **`glTF Binary (.glb)`** | 1 file ringkas berisi mesh, skeleton, animasi & tekstur |
| **Remember Export Settings** | `ON` | Memudahkan export berulang kali |

### Section `Include`
| Pengaturan | Nilai | Keterangan |
| :--- | :--- | :--- |
| **Limit to Selected Objects** | `OFF` *(atau `ON` jika pilih Armature + Mesh)* | Pastikan **Armature DAN semua Mesh** ikut ter-export |
| **Data → Cameras / Punctual Lights** | `OFF` | Jangan ikutkan kamera/lampu Blender ke model 3D |

### Section `Transform`
| Pengaturan | Nilai | Keterangan |
| :--- | :--- | :--- |
| **+Y Up** | **`ON`** *(default)* | Standar koordinat Bevy 3D |

### Section `Data → Mesh` & `Material`
| Pengaturan | Nilai | Keterangan |
| :--- | :--- | :--- |
| **Apply Modifiers** | `OFF` *(penting untuk Rig/Armature!)* | Jika `ON`, Armature modifier bisa ter-bake jadi statis |
| **UVs / Normals / Vertex Colors** | `ON` | Standar rendering PBR Bevy |
| **Materials** | **`Export`** | Pastikan tekstur ikut terbawa |
| **Images** | **`Automatic`** *(atau `JPEG`/`WebP` jika tekstur > 2K)* | Agar ukuran file tetap ringan |
| **Compression (Draco)** | **`OFF` (JANGAN DICENTANG!)** | Bevy tidak memakai Draco; TWG otomatis buat **LOD2** di server |

### Section `Data → Armature` & `Skinning`
| Pengaturan | Nilai | Keterangan |
| :--- | :--- | :--- |
| **Export Deformation Bones Only** | `OFF` *(atau `ON` untuk Mixamo standar)* | `OFF` paling aman untuk semua rig |
| **Use Rest Position (Exporting Skinning)** | `OFF` | Biarkan default |

### Section `Animation`
| Pengaturan | File `Idle` (`*_idle.glb`) | File `Walk` & `Aksi Tambahan` |
| :--- | :--- | :--- |
| **Animation (Centang utama)** | **`OFF` (MATIKAN)** | **`ON` (AKTIFKAN)** |
| **Animation Mode** | — | **`Actions`** *(atau `Active Actions`)* |
| **Always Sample Animations** | — | **`ON` (WAJIB!)** — mencegah animasi Mixamo patah/kaku di Bevy |
| **Sampling Rate** | — | `1` *(setiap frame)* |
| **Bake All Objects Animations** | — | `OFF` |
| **Optimize Animation Size** | — | `ON` |
| **Force Keep Channels for Objects** | — | `OFF` |
| **Group by NLA Track** | — | `OFF` |

> **Kenapa dulu Export GLB Blender gagal di MapTalks tapi sekarang sukses di Bevy?**
> Di MapTalks dulu sering bermasalah dengan material/tangents/skinning non-normalisasi dari Blender, sehingga harus lewat Assimp. Di **TWG Rust + Bevy**, renderer menggunakan parser `bevy_gltf` resmi **plus** server TWG secara otomatis menjalankan **normalisasi bobot tulang (`WEIGHTS_0`)** dan membuat versi ringan **LOD2** setiap kali file `.glb` diunggah!

---

## 4. Cara Upload & Menggunakan di Aplikasi TWG

### Langkah 1 — Upload Model Utama (`Idle` + `Walk`)
1. Buka menu **`Assets → 3D Models`** (login sebagai Admin).
2. Klik **`+ Tambah Model`**, pilih tipe **`Animasi (2 file: Idle + Walk)`**.
3. Masukkan:
   - **File Idle**: `*_idle.glb` *(tanpa animasi)*
   - **File Walk**: `*_walk.glb` *(animasi jalan)*
4. Klik **`Upload & Simpan ke Server`**.

### Langkah 2 — Upload Aksi Tambahan (`+ Tambah Aksi`)
1. Klik model beranimasi yang baru dibuat (atau model Animasi yang sudah ada) di daftar kiri `Assets → 3D Models`.
2. Di panel kanan, buka bagian **`Animasi Aksi Tambahan`**.
3. Isi **Nama Aksi** (contoh: `Melambai`, `Menembak`, `Pingsan`, `Hormat`).
4. Pilih file `.glb` hasil export Blender, lalu klik **`+ Upload Aksi ke Model`**.
5. Klik tombol **`Preview`** di samping aksi yang baru diunggah untuk langsung melihat animasinya berputar di studio 3D!
6. Ulangi untuk aksi lainnya sebanyak yang dibutuhkan — semua file otomatis tersinkron ke seluruh klien & Layar Besar.

### Langkah 3 — Pasang di Skenario (`Animasi → Lainnya`)
1. Masuk ke **Plot Skenario**, tempatkan model karakter 3D tersebut di peta.
2. Buka tab **`Animasi → Lainnya`**, lalu pada kartu **`Aksi Model 3D (Klip Animasi Tambahan)`**:
   - Isi nama animasi (mis. `Polisi Atur Lalin`, `Polisi Menembak`, `Massa Pingsan`).
   - Klik **`+ Pilih Objek 3D di Peta`**, lalu klik karakter 3D di peta.
3. Di **Inspector (panel kanan)**, pilih **Klip Aksi Animasi** dan **Mode Putar**:
   - **`Loop Terus (Sampai Stop)`** → cocok untuk polisi *melambai mengatur lalu lintas*.
   - **`Sekali & Tahan Pose Akhir`** → cocok untuk karakter *pingsan* (berdiri → rebah, lalu **tetap rebah** sampai tombol `Stop` ditekan atau karakter diperintahkan jalan).
   - **`Sekali & Kembali ke Idle`** → cocok untuk *menembak* atau *hormat*.
   - **`Ulangi N× lalu Kembali ke Idle`** → mengulang gerakan sejumlah `N` kali lalu kembali diam.
4. Atur **Kecepatan**, **Jeda Awal (Delay)**, dan pastikan opsi **`Otomatis Stop & pindah ke Walk saat Rute berjalan`** tetap tercentang:
   - Saat karakter tersebut mulai bergerak mengikuti **Rute / Formasi / RTS**, aksi otomatis berhenti (`Stop`) dan karakter beralih mulus ke animasi **`Walk`**.
   - Ketika rute selesai, karakter berhenti di **`Idle`** (jika ingin memutar aksinya lagi, cukup klik **`Play`** kembali).
5. **Tips Kombinasi Skenario:**
   - **Menembak + Peluru**: Buat animasi **`Grup → Combine`** berisi `Aksi Model 3D (Menembak)` + `Shoot (Peluru/Gas Air Mata)` agar gerakan tangan & efek tembakan berjalan serentak!
   - **Jalan lalu Pingsan / Melambai**: Buat animasi **`Grup → Sequence`** berisi `Rute` → diikuti `Aksi Model 3D` agar karakter berjalan dulu ke lokasi, berhenti, lalu otomatis melakukan aksinya!

---

## 5. Checklist Troubleshooting Cepat

| Masalah | Penyebab Umum | Solusi |
| :--- | :--- | :--- |
| **Error "Model idle tidak boleh memiliki animasi"** | Centang `Animation` di Blender masih `ON` saat export `*_idle.glb` | Export ulang dengan **`Animation = OFF`** |
| **Error "wajib memiliki animasi, ditemukan 0 animasi"** | Centang `Animation` masih `OFF` atau Action belum aktif saat export `*_walk.glb` / aksi | Pastikan Action aktif di Action Editor & **`Animation = ON`** |
| **Error "Model menggunakan kompresi Draco"** | Opsi `Compression` di Blender glTF Exporter dicentang | Matikan **`Compression (Draco) = OFF`** lalu export ulang |
| **Karakter baru / Walk / Aksi terlihat terlalu kecil atau terlalu besar** | Model lama (Assimp/FBX) berskala cm (`100×`), sedangkan export Blender `.glb` berskala meter (`1×`) | TWG sekarang **otomatis** menyesuaikan skala awal saat model baru di-drop ke peta (~10 m, seragam dengan katalog lama) **dan** otomatis menyamakan skala `Walk` & `Aksi Tambahan` mengikuti tinggi `Idle` (tidak perlu scale manual `100×` di Blender) |
| **Karakter miring / melayang / terbenam saat Aksi diputar** | Origin / rotasi file aksi berbeda dengan file `Idle` | Pastikan file `Idle`, `Walk`, dan `Aksi` berasal dari mesh/rig Mixamo yang sama, kaki di `Z = 0`, hadap `-Y` (Blender) |
| **Karakter meluncur menjauh saat Walk / Aksi** | Opsi `In Place` di Mixamo lupa dicentang | Download ulang dari Mixamo dengan **`In Place = ON`** |
| **Animasi patah-patah / tulang berantakan di Bevy** | `Always Sample Animations` mati atau `Apply Modifiers` menyala | Export ulang dengan **`Always Sample Animations = ON`** dan **`Apply Modifiers = OFF`** |
| **Ukuran `.glb` terlalu besar (> 15 MB)** | Tekstur dari Tripo3D beresolusi 4K PNG | Kecilkan tekstur ke `1024×1024` atau `2048×2048` di Blender, atau pilih format Image `JPEG/WebP` saat export `.glb` |
