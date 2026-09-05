# Predator Sense for Linux (Türkçe)

<p align="center">
  <a href="README.md">🇺🇸 Read in English</a> · <a href="README-ptbr.md">🇧🇷 Leia em Português</a>
</p>

<p align="center">
  <img src="predator-sense-gui/resources/logo.jpeg" width="120" alt="Predator Sense Logo">
</p>

<p align="center">
  <b>Acer oyuncu dizüstü bilgisayarları için resmi olmayan Linux çekirdek modülü ve donanım kontrol arayüzü</b><br>
  <i>RGB Klavye Aydınlatması &bull; Turbo Modu &bull; Sıcaklık İzleme &bull; Performans Profilleri</i>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Dil-Rust-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/GTK-4-blue?logo=gtk" alt="GTK4">
  <img src="https://img.shields.io/badge/Userspace-%25100_Rust-orange?logo=rust" alt="Userspace %100 Rust">
  <img src="https://img.shields.io/badge/Lisans-GPL--3.0-green" alt="License">
  <img src="https://img.shields.io/badge/Platform-Linux-yellow?logo=linux" alt="Linux">
</p>

<p align="center">
  Oluşturan ve geliştiren: <a href="https://github.com/cleyton1986">Cleyton Alves</a>
</p>

---

## Yasal Uyarı

> **Uyarı**
> **Kullanım tamamen kendi sorumluluğunuzdadır!** Bu **resmi olmayan** bir projedir. Acer, geliştirilmesinde yer almamıştır. Çekirdek modülü, resmi PredatorSense Windows uygulamasının tersine mühendisliği yapılarak geliştirilmiştir. Bu sürücü, tüm dizüstü serilerinde test edilmemiş düşük seviyeli WMI/ACPI yöntemleriyle etkileşime girer. Yazarlar, donanımınıza gelebilecek herhangi bir zarardan sorumlu değildir.

> **Not**
> Bahsi geçen tüm ticari markalar, ürün adları ve logolar (Acer, Predator, PredatorSense, Helios, Nitro, AeroBlade, CoolBoost) kendi sahiplerinin (Acer Inc.) mülkiyetindedir. Bu proje, Acer Inc. ile hiçbir şekilde bağlantılı, onaylı veya sponsorlu değildir.

> **Ürün görselleri**
> `predator-sense-gui/resources/models/` altındaki dizüstü fotoğrafları, resmi Acer Predator/Nitro ürünlerini gösterir ve yalnızca uygulamanın, kullanıcının kendi makinesinde algılanan modeli görsel olarak tanımlayabilmesi için kullanılır (sistemin DMI/BIOS'unun bildirdiği `product_name` ile eşleştirilerek). Bu görseller **bu projenin GPLv3 lisansı kapsamında değildir** — ürün fotoğraflarının telif hakkı Acer Inc.'e ve/veya orijinal yaratıcılarına aittir. Bu görseller, iyi niyetle, ticari olmayan, tamamen bilgilendirme amaçlı bir temelde (isimlendirme/ürün tanımlama amaçlı kullanım) ve bu proje adına herhangi bir mülkiyet iddiası olmaksızın burada yer almaktadır. Hak sahibiyseniz ve bir görselin kaldırılmasını istiyorsanız lütfen bir issue açın, derhal kaldırılacaktır.

Bu uygulama, Acer bir dizüstü bilgisayardan Linux'ta en iyi verimi almak için **kişisel kullanım** amacıyla oluşturuldu — çünkü Acer, PredatorSense için resmi Linux desteği sunmuyor. Aynısını isteyen herkes için ücretsiz olarak paylaşılıyor.

Bu uygulama/proje işinize yaradıysa ve/veya bir şekilde beğendiyseniz, bir yıldız bırakmayı düşünün, çok yardımcı oluyor ⭐

---

## Ekran Görüntüleri

<p align="center"><b>Dashboard</b> — Dizüstü fotoğrafı ve tek bakışta tüm sistem özellikleri: CPU, GPU, RAM, depolama, ağ ve işletim sistemi.</p>
<p align="center"><img src="assets/psense-1.png" width="800" alt="Dashboard"></p>

<p align="center"><b>Sıcaklıklar</b> — CPU, GPU, sistem, NVMe diskleri, WiFi ve RAM için tek ekranda canlı göstergeler.</p>
<p align="center"><img src="assets/psense-2.png" width="800" alt="Sıcaklıklar"></p>

<p align="center"><b>Kullanım</b> — En çok kaynak tüketen süreçler, animasyonlu çubuklar ve tıkla-genişlet detaylarıyla CPU, GPU, bellek ve depolama (sıcaklık göstergesinde CSS tabanlı alev animasyonuyla).</p>
<p align="center"><img src="assets/psense-3.png" width="800" alt="Kullanım"></p>

<p align="center"><b>Ağ</b> — Zirve takibi ve otomatik arayüz algılamasıyla (Wi-Fi veya Ethernet) gerçek zamanlı indirme/yükleme grafikleri.</p>
<p align="center"><img src="assets/psense-4.png" width="800" alt="Ağ"></p>

<p align="center"><b>Aydınlatma</b> — Bölge bazlı statik (4 bölge) ve dinamik RGB klavye efektleri (Breathing, Neon, Wave, Shifting, Zoom).</p>
<p align="center"><img src="assets/psense-5.png" width="800" alt="Aydınlatma"></p>

<p align="center"><b>Modlar</b> — Performans profilleri: Sessiz, Dengeli, Performans ve Turbo, artı yalnızca pilde kullanılabilen bir Eco katmanı (CPU governor + Intel EPP + GPU güç limiti).</p>
<p align="center"><img src="assets/psense-6.png" width="800" alt="Modlar"></p>

<p align="center"><b>GameSync</b> — Bir oyun ve profilini kaydedin; oyun çalışırken uygulama otomatik olarak o profile geçer, oyun kapandığında önceki profili geri yükler.</p>
<p align="center"><img src="assets/psense-15.png" width="800" alt="GameSync"></p>

<p align="center"><b>Fan Kontrolü</b> — Animasyonlu dönen fanlarla canlı RPM, CoolBoost anahtarı ve Auto/Max modları.</p>
<p align="center"><img src="assets/psense-7.png" width="800" alt="Fan Kontrolü"></p>

<p align="center"><b>Pil</b> — Şarj yüzdesi, voltaj, akım, güç, döngü sayısı, sağlık durumu, üretici ve uzun ömür için %80 şarj limiti.</p>
<p align="center"><img src="assets/psense-8.png" width="800" alt="Pil"></p>

<p align="center"><b>GPU</b> — Canlı grafikler, saat hızları, kullanım, VRAM, güç tüketimi ve PCIe bilgileriyle NVIDIA paneli.</p>
<p align="center"><img src="assets/psense-9.png" width="800" alt="GPU"></p>

<p align="center"><b>Grafikler</b> — Min/maks takibiyle detaylı CPU ve GPU geçmiş grafikleri.</p>
<p align="center"><img src="assets/psense-10.png" width="800" alt="Grafikler"></p>

<p align="center"><b>Yapay Zeka Asistanı (beta)</b> — Ollama destekli yerel yapay zeka asistanı: sohbet, model yöneticisi (yüklü modelleri listele, yenilerini indir, hangisinin çalışacağını seç), düşünürken canlı VRAM/GPU kaynak kullanımı ve kalıcı bir eylem günlüğü.</p>
<p align="center"><img src="assets/psense-11.png" width="800" alt="Yapay Zeka Asistanı"></p>

<p align="center"><b>Sürücüler ve kılavuzlar</b> — Seri numarasını (kopyalama düğmesiyle) ve Acer'ın resmi sürücüler-ve-kılavuzlar sayfasına doğrudan bir bağlantıyı gösterir, ayrıca dizüstünde seri numarası etiketinin nerede olduğunu gösteren bir çizim içerir.</p>
<p align="center"><img src="assets/psense-16.png" width="800" alt="Sürücüler ve kılavuzlar"></p>

<p align="center"><b>Ayarlar</b> — Sistem tepsisine küçültme, sistemle başlatma, başlangıçta profili otomatik uygulama, dil tercihleri ve modele özgü desteklenen özellikler listesi.</p>
<p align="center"><img src="assets/psense-12.png" width="800" alt="Ayarlar"></p>

<p align="center"><b>Kapak logosu aydınlatması</b> — Renk destekli kapak logosuna sahip modellerde, ekranın arkasındaki logo için bağımsız RGB kontrolü (Static/Breathing/Neon). Çalışma zamanında algılanır: kontrol yalnızca donanım bir yetenek sorgusuna yanıt verirse görünür, bu özelliğe sahip olmayan modellerde güvenle gizli kalır.</p>
<p align="center"><img src="assets/psense-13.png" width="800" alt="Kapak logosu aydınlatması"></p>
<p align="center"><img src="assets/psense-14.jpg" width="800" alt="Bir Predator PHN16-73 üzerinde yeşil yanan kapak logosu"></p>
<p align="center"><sub>Bu özellik <a href="https://github.com/jlucaso1">@jlucaso1</a> tarafından katkı olarak eklendi ve kendi Predator PHN16-73 cihazında test edildi. Bu dizüstünün kapak logosu renk desteklemiyor, bu yüzden özellik onun donanımı kullanılarak doğrulandı.</sub></p>

---

## Hakkında

Acer oyuncu dizüstü bilgisayarları için RGB klavye aydınlatması ve Turbo modu sağlayan resmi olmayan Linux çekirdek modülü (Acer Predator, Acer Helios, Acer Nitro).

[JafarAkhondali](https://github.com/JafarAkhondali) ve katkıda bulunanların [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) projesinden ilham alınmış ve bu proje üzerine kurulmuştur. Bu proje, Acer oyun fonksiyonlarını desteklemek için mevcut Linux Acer-WMI çekirdek modülünü genişletir ve Rust ile GTK4 kullanılarak geliştirilmiş **eksiksiz bir masaüstü GUI uygulaması** ekler.

---

## Özellikler

| Özellik | Açıklama |
|---------|-------------|
| **Dashboard** | Dizüstü fotoğrafı + eksiksiz sistem özellikleri (CPU, GPU, RAM, depolama, ağ, işletim sistemi) |
| **Sıcaklıklar** | CPU, GPU, sistem, NVMe, WiFi ve RAM için canlı göstergeler |
| **Kullanım** | 4 sekmeli görünüm: CPU / GPU / Bellek / Depolama, en çok kaynak tüketen süreçler, tıkla-genişlet detaylar ve sıcaklık göstergelerinde CSS tabanlı alev animasyonu |
| **Ağ** | Zirve takibi ve otomatik arayüz algılamasıyla gerçek zamanlı indirme/yükleme grafikleri |
| **RGB Klavye Kontrolü** | WMI üzerinden bölge bazlı statik (4 bölge) ve dinamik efektler (Breathing, Neon, Wave, Shifting, Zoom). Çekirdek modülü olmayan donanımlarda RGB, bunun yerine USB/I2C-HID üzerinden yerel olarak çalışır — ENEK5130 çip (4 bölgeli statik, Breathing/Neon), 2024+ Sunrex çip (tek bölge, tam efekt listesi) veya Chicony çip (7 renkli palet, Helios 300) — otomatik algılanır, bkz. [Uyumluluk](#uyumluluk) |
| **RGB Kapak Logosu** | Ekranın arkasındaki amblem için bağımsız açma/kapama, düz renk, parlaklık, Breathing ve Neon kontrolleri, canlı vektör önizlemesiyle. Yalnızca çalışma zamanı HID yetenek algılamasından sonra gösterilir |
| **Performans Profilleri** | Sessiz / Dengeli / Performans / Turbo modları, artı yalnızca pilde kullanılabilen bir Eco katmanı (CPU governor + Intel EPP + GPU güç limiti) |
| **Fan Kontrolü** | Animasyonlu dönen fanlarla canlı RPM, CoolBoost anahtarı, Auto/Max modları, artı deneysel fan başına PWM kontrolü ve otomatik sıcaklık eğrisi (destekleniyorsa) |
| **Pil** | Şarj istatistikleri, döngü sayısı, sağlık durumu, üretici bilgisi ve uzun ömür için %80 şarj limiti |
| **GPU Paneli** | NVIDIA metrikleri: sıcaklık, kullanım, VRAM, saat hızları, güç tüketimi, canlı grafiklerle PCIe bilgisi, artı bir **güç limiti (TGP) kaydırıcısı** |
| **Grafikler** | Min/maks takibiyle detaylı CPU ve GPU geçmiş grafikleri |
| **Yapay Zeka Asistanı** 🧪 | [Ollama](https://ollama.com) destekli, isteğe bağlı yerel yapay zeka asistanı — canlı donanım durumunu okur ve sabit, önceden doğrulanmış bir eylem kümesi üzerinden değişiklikler önerir veya uygular (termal profil, fan modu, CoolBoost, RGB, GPU güç limiti, pil). Sohbet, model yöneticisi (indir/seç), canlı kaynak/VRAM izleyicisi ve kalıcı bir eylem günlüğü. Otomatik uygula ya da her seferinde onay iste, seçim sizin. Ollama'nın ayrıca kurulması gerekir — aşağıda [Yapay Zeka Asistanı](#yapay-zeka-asistanı-beta) bölümüne bakın |
| **Otomatik yetenek algılama** | Her modelin neyi desteklediğini algılar ve arayüzü buna göre uyarlar — desteklenmeyen özellikler hata vermek yerine "bu modelde kullanılamıyor" olarak gösterilir. Desteklenen özellikler Ayarlar'da listelenir |
| **Sıcaklık uyarıları** | CPU/GPU 90°C'yi aştığında masaüstü bildirimi (sistem tepsisindeyken de çalışır) |
| **Otomatik güç profili** | Şarj/pil durumu değiştiğinde profili otomatik değiştirir — her durum için hedef profil Ayarlar'da yapılandırılabilir (varsayılan: şarjda Performans, pilde Dengeli) |
| **Hata ayıklama günlüğü** | Ayarlar'da isteğe bağlı anahtar — daemon ve uygulama olaylarını uzaktan sorun giderme için `~/.local/share/predator-sense/` altına kaydeder (döngülü, 5MB×3). Varsayılan olarak kapalı |
| **Sistem Tepsisi** | Predator simgesiyle sistem tepsisine küçültme — uygulama arka planda çalışmaya devam eder |
| **PredatorSense Tuşu** | Donanım tuşu eşlemesi — NumLock yanındaki tuş uygulamayı açar |
| **DKMS** | Çekirdek modülleri, çekirdek güncellemelerinde otomatik olarak yeniden derlenir |
| **Uluslararasılaştırma** | Sistem diline göre otomatik İngilizce / Portekizce |
| **Oyuncu Arayüzü** | Nabız gibi atan neon çubuklar, kesikli dairesel göstergeler, çokgen panel kenarlıklarıyla koyu tema. Vurgu rengi, algılanan markaya göre otomatik olarak değişir — Predator/Helios/Triton'da camgöbeği, Nitro'da turuncu/kırmızı (NitroSense ile uyumlu) — elle değiştirilecek bir ayar yok |

---

## Uyumluluk

**Benim dizüstümde çalışır mı?**

Açıklama: ✅ test edildi ve çalışıyor · 🟡 uygulandı, test edilmedi (test edecek birine ihtiyaç var) · 🧪 deneysel (test edecek birine ihtiyaç var) · ❌ çalışmıyor · `-` uygulanamaz

| Ürün Adı | Turbo (Uygulama) | Turbo (Test) | RGB (Uygulama) | RGB (Test) | Fan RPM okuma | Fan profilleri | Fan PWM % |
|--------------|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| AN16S-61 | - | - | ✅ | ✅ | ❌ | - | ❌ |
| AN515-45 | - | - | ✅ | ✅ | ❌ | - | ❌ |
| AN515-55 | - | - | ✅ | ✅ | ❌ | - | ❌ |
| AN515-56 | - | - | ✅ | ✅ | ❌ | - | ❌ |
| AN515-57 | - | - | ✅ | ✅ | ❌ | - | ❌ |
| AN515-58 | ✅ | 🟡 | ✅ | ✅ | 🟡 | 🟡 | 🧪 |
| AN517-41 | - | - | ✅ | ✅ | ❌ | - | ❌ |
| PH16-71 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |
| PH16-72 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | 🟡 | 🧪 |
| PH315-52 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PH315-53 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| **PH315-54** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| PH315-55 | ✅ | 🟡 | ✅ | ❌ | 🟡 | - | ❌ |
| PH317-53 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PH317-54 | ✅ | ✅ | ✅ | 🟡 | ✅ | - | 🧪 |
| PH317-55 | - | - | ✅ | 🟡 | ❌ | - | ❌ |
| PH317-56 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |
| PH517-51 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |
| PH517-52 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |
| PH517-61 | ✅ | 🟡 | ✅ | ✅ | 🟡 | - | ❌ |
| PHN16-71 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |
| PHN16S-71 | ✅ | ✅ | ✅ | ✅ | ✅ | - | ❌ |
| PHN16-72 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🧪 |
| **PHN16-73** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| PHN18-71 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PT314-51 | ❌ | ❌ | ✅ | ✅ | 🟡 | - | ❌ |
| PT314-52s | ✅ | ✅ | ✅ | 🟡 | 🟡 | - | ❌ |
| PT315-51 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PT316-51 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PT515-51 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PT516-52s | ✅ | 🟡 | ✅ | ✅ | 🟡 | - | ❌ |
| PT917-71 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |

> Modeliniz listede yoksa yine de çalışabilir — çekirdek modülü uyumlu WMI arayüzlerini otomatik olarak algılar. Sizde çalıştıysa (ya da çalışmadıysa), lütfen modelinizi belirterek bir issue açın ki bu tabloyu güncelleyebilelim.

### Fan kontrolü — üç seviye

| Seviye | Ne yapar | Kullanılabilirlik |
|---|---|---|
| **Fan RPM okuma** | CPU/GPU fan hızını okur (`fan1_input`, `fan2_input`) | Çoğu oyuncu model (otomatik algılanır) |
| **Fan profilleri** | `platform_profile` üzerinden Sessiz / Dengeli / Performans / Turbo | `predator_v4` modelleri |
| **Fan PWM %** 🧪 | Fan başına hız kontrolü (`pwm1`/`pwm2` %0–100), mainline `acer-wmi`'den WMI üzerinden taşındı — **yalnızca çekirdek ≥ 6.14** | `ACER_CAP_PWM` özellikli model alt kümesi (AN515-58, PHN16-72/73, PH16-72, …) |

> **🧪 PWM fan kontrolü deneyseldir.** Üst akış Linux çekirdeği `acer-wmi` sürücüsünden taşınmıştır ve güvenli WMI yöntemleri kullanır (ham EC yazımı yoktur), ancak proje sahibi tarafından **gerçek donanımda doğrulanmamıştır** (kendisinde PWM'i olmayan bir PH315-54 var). Desteklenen bir modeliniz varsa, test raporları çok memnuniyetle karşılanır. **Kullanım tamamen kendi sorumluluğunuzdadır** — yukarıdaki yasal uyarıya bakın.

### Alternatif: linuwu_sense (quirk tablosunda olmayan, Turbo'su çalışmayan donanımlar)

`facer`'ın `enable_all=1` yedek modu, Acer WMI destekli her kartı tanır, ancak tam `predator_v4` profil kümesi (yazılabilir `turbo_state` dahil `balanced-performance`/`performance` içeren 5 profil) yalnızca kendi DMI quirk tablosunda bulunan kartlarda geçerlidir. Quirk tablosunda olmayan bir kartta `platform_profile_choices` yalnızca `low-power quiet balanced` ile sınırlı kalır ve firmware daha fazlasını desteklese bile `turbo_state` salt okunur kalır — bu durum bir PHN16-73 biriminde (Macan_ARX, BIOS V1.26) [#33](https://github.com/cleyton1986/predator-sense/issues/33) numaralı issue'da bildirilmiştir.

Bu sizin durumunuzsa, topluluk tarafından geliştirilen [Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense) modülü (`predator_v4=1` ile yüklenir), bu uygulamanın zaten doğrudan okuduğu aynı genel `platform_profile`/`intel_pstate`/`acer-wmi-battery` arayüzleri üzerinden tam profil kümesini sunar — `facer`'a özgü herhangi bir kod yolu devreye girmez. `v0.2.71-preview` sürümünden itibaren uygulama `linuwu_sense`'i algılar ve bu arayüzleri gerçekten sağlayan sürücü oysa "facer'ı kur" uyarısını atlar. RGB ve termal profil kalibrasyonu (ikisi de yalnızca `facer` ile çalışır, yukarıya ve aşağıya bakın) linuwu_sense altında hâlâ kullanılamaz durumda kalır.

### Çekirdek modülü olmadan RGB (yalnızca I2C-HID donanımlar)

Bazı modeller (doğrulanmış: PHN16S-71, PHN16-73, AN16S-61) klavyenin RGB denetleyicisini `facer.ko` WMI arayüzü yerine ayrı bir I2C-HID çipi (ENEK5130) üzerinden yönlendirir — uygulama onunla `/dev/hidrawN` üzerinden doğrudan konuşur, bu yüzden çekirdek modülü hiç yüklü olmasa bile bunlar çalışır:

| Özellik | Durum |
|---|---|
| Bölge bazlı statik renk, parlaklık, arka ışığı kapatma | ✅ çalıştığı doğrulandı (PHN16S-71, AN16S-61) |
| Dinamik efektler — Breathing, Neon | ✅ çalıştığı doğrulandı (PHN16S-71, AN16S-61) — yerel, tek bir HID yazımı, donanım deseni kendi başına döngüye alır. PHN16S-71 biriminde Breathing seçilen rengi yok sayıp gökkuşağı döngüsüne giriyor; başka donanımlarda farklı olabilir |
| Dinamik efektler — Wave, Shifting, Zoom | Yalnızca ekranda önizleme (donanıma yazım yok) — bu efektlerin kodlarının donanım nesilleri arasında farklı anlamlara geldiği tespit edildiği için henüz bağlanmadılar |
| RGB kapak logosu — kapalı, düz renk, parlaklık, Breathing, Neon | ✅ çalıştığı doğrulandı (PHN16-73) |

Kapak logosu desteği, model adına dayalı bir izin listesinden etkinleştirilmiyor. Arayüz gösterilmeden önce denetleyicinin A1 hedef raporunda `0x83` hedefini bildirmesi ve buna karşılık gelen, boş olmayan A3 yeteneklerini döndürmesi gerekir; uygulama bu kontrolü her yazımdan hemen önce tekrarlar. Hotkey daemon'u yalnızca uygulamanın giriş ve uykudan dönüş sonrasında daha önce başarıyla uyguladığı bir ayarı geri yükler ve kayıtlı bir ayar yoksa ya da hedef mevcut değilse logoyu tamamen atlar.

[AN16S-61 hakkında bağımsız bir rapor](https://github.com/cleyton1986/predator-sense/issues/31) (ayrıca raporu paylaşanın kendi [bağımsız protokol aracına](https://github.com/ArnarValur/Nitro16S-AI-RGB-Keyboard) da bakın), statik/Breathing/Neon/Wave dışında altı yerel kablo modu daha (bir donanım kapatma modu, EC'nin kendisinin tetiklediği bir açılış-yanıp-sönme modu ve dört yerleşik animasyon daha) ile bir mod/turbo tuşu LED hedefi eşledi. Bunların hiçbiri henüz uygulamaya bağlanmadı — bunun için önce yerel donanım efekt kodlarına ayrılmış bir alan tanımlanması gerekiyor, bu yüzden gelecekteki bir iyileştirme olarak takip ediliyor.

Aynı rapor, denetleyiciden doğrudan alınan çözümlenmiş bir HID report descriptor da içeriyordu; bu da gerçek bir hatayı ortaya çıkardı: uygulama, A3 yetenek raporundaki bölge sayısını yanlış bayttan (`byte[3]`, hedef sınıfı başına sabit bir sabit) okuyordu; oysa denetleyicinin kendi descriptor'ının bunun için bildirdiği bayt `byte[4]`'tü. Bu, hem uygulamada hem de hotkey daemon'unda `v0.2.69-preview` sürümünde düzeltildi. Bu, modele özgü bir değişiklik değil, protokol seviyesinde bir düzeltmedir — report descriptor alan yerleşimi çipin kendi firmware'inden gelir (doğrulanan üç modelin hepsinde aynı `0CF2:5130` çipi) — ve önceki değer her zaman doğru değerin daha kapsayıcı bir üst kümesi olduğundan, zaten çalıştığı doğrulanmış donanımlarda hiçbir kablo baytını değiştirmez.

### 2024+ donanımlarda RGB (Sunrex/Darfon USB HID)

Daha yeni bir nesil (PH16-72 ve aynı USB HID çiplerini paylaşan diğer 2024-2026 modelleri, bkz. issue #26), klavye ve kapak logosu RGB'sini hem WMI'dan hem de yukarıdaki ENEK5130 çipinden tamamen farklı bir denetleyici çiftine taşıdı — klavye için Sunrex `05af:*`, logo için Darfon `0d62:*`. Uygulama bunları da doğrudan algılayıp yönetiyor, mevcut olduklarında ENEK5130/WMI yollarının önüne otomatik olarak seçiliyor:

| Özellik | Durum |
|---|---|
| Klavye: Kapalı, Statik, Breathing, Wave, Snake, Neon, Spot, Star, Rainbow, 5× Slash, Zoom, Row Wave, Swiping | 🟡 uygulandı, gerçek donanımda onay bekleniyor |
| Kapak logosu: kapalı, düz renk, parlaklık, Breathing | 🟡 uygulandı, gerçek donanımda onay bekleniyor |

Bu çipin bağımsız bölgeleri yok — yukarıdaki 4 bölgeli ENEK5130 denetleyicisinin aksine, tüm klavye aynı anda tek bir renk/efekt kullanır. Kablo protokolü, resmi Windows uygulamasının derlenmiş iki ayrı sürümünden byte byte tersine mühendislikle çıkarıldı (her sabit bayt dizisi ve checksum formülü ikisi arasında tam olarak eşleşti), tahmin edilmedi — ama henüz kimse gerçek donanımda doğrulamadı, bu yüzden gerçek bir rapor gelene kadar test edilmemiş kabul edin.

Üçüncü bir çip (Chicony, Helios 300/PH317-56), topluluk tarafından tersine mühendislikle belgelenen ([NT411/Acer-Predator-Fan-RGB-Controller-Linux](https://github.com/NT411/Acer-Predator-Fan-RGB-Controller-Linux)) başka bir USB HID protokolü kullanır ve burada o spesifikasyondan yeniden uygulanmıştır — 12 efekt üzerinde sabit 7 renkli bir palet (keyfi RGB değil, bir donanım/firmware kısıtlaması). Bu da 🟡, onay bekliyor.

### Zaten Linuwu-Sense veya DAMX mi kullanıyorsunuz?

[Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense) (ve bunun üzerine kurulu [DAMX](https://github.com/PXDiv/Div-Acer-Manager-Max)), Linux'ta Acer Predator/Nitro donanımını yöneten ayrı, ilgisiz bir projedir. Bu projenin bir bağımlılığı değildir ve buradan hiçbir kodu kullanılmaz — ama çekirdek modülü, `facer`'ın ihtiyaç duyduğu **aynı WMI GUID'lerini** talep eder ve çekirdek, aynı anda aynı cihazı iki sürücünün talep etmesine izin vermez.

Yükleyici, `linuwu_sense`'in zaten yüklü veya DKMS ile kurulu olduğunu algılarsa, otomatik olarak **mevcut kurulumunuza dokunmaz** — `acer_wmi`'yi kara listeye almaz veya `facer`'ı zorla yüklemez, böylece zaten çalışan bir Linuwu-Sense/DAMX kurulumuyla çakışmaz (veya onu bozmaz). Hangi platform sürücüsü aktif olursa olsun klavye RGB'si bu uygulama üzerinden HID yolu ile (yukarıya bakın) çalışmaya devam eder; bu durumda fan/termal kontrolü halihazırda kullandığınız araçla kalır.

---

## Kurulum

### Hazır Yükleyici (En Hızlı)

Release yükleyicisini doğrudan indirip çalıştırın:

```console
curl --fail --location https://github.com/cleyton1986/predator-sense/releases/latest/download/predator-sense-installer --output predator-sense-installer
chmod +x predator-sense-installer
sudo ./predator-sense-installer --install
```

Yükleyici, ayrıcalıklı yardımcı, hotkey dinleyicisi ve tray servisinin hepsi aynı Rust multicall binary'si tarafından sağlanır. Yükleyici, bir shell script bootstrap'i olmadan her şeyi indirip yapılandırır.

### Etkileşimli Yükleyici (hazır binary, Rust araç zinciri gerekmez)

`predator-sense-installer` binary'sini [Releases](../../releases) sayfasından indirin. Bu bağımsız bir Rust binary'sidir, bir paket değildir — yine de uygulamanın kaynağını (çekirdek modülü için) ve eşleşen hazır release binary'sini indirmek için internet erişimine ihtiyaç duyar, ancak Rust kurmayı ve GTK4 uygulamasını makinenizde derlemeyi tamamen atlar:

```console
chmod +x predator-sense-installer
sudo ./predator-sense-installer
```

**1. seçeneği** (Tam Kurulum) seçin. Yükleyici otomatik olarak:

1. Dağıtımınızı algılar (Debian/Ubuntu/Mint, Fedora, Arch)
2. Sistem bağımlılıklarını kurar (GTK4, libadwaita, derleme araçları, çekirdek başlıkları)
3. Eşleşen release'in kaynağını + hazır release binary'sini indirir
4. `facer` çekirdek modülünü derler ve yükler (bu kısım her zaman yerel olarak derlenir — çekirdek modülleri farklı çekirdek sürümleri arasında hazır olarak dağıtılamaz)
5. Simgeli masaüstü menü girdisi oluşturur
6. PredatorSense donanım tuşunu eşler (girişte otomatik başlar)
7. Sistem tepsisi desteğini kurar

Hazır yol, hedef makinede Rust/cargo gerektirmez. Yükleyici ayrıca, durum kontrolleri, çekirdek modülü yeniden yüklemeleri, güncellemeler ve kaldırma için bağımsız bir yönetim aracı olarak `/opt/predator-sense/` altına kopyalanır (bkz. [Yükleyici Seçenekleri](#yükleyici-seçenekleri)).

Kurulumdan sonra uygulamayı şu şekillerde açabilirsiniz:
- **PredatorSense tuşuna** basarak (NumLock yanındaki tuş)
- Uygulama menünüzde **"Predator Sense"** araması yaparak
- Bir terminalde `/opt/predator-sense/predator-sense` çalıştırarak

### Manuel Kurulum (Kaynaktan derleme)

#### Ön Koşullar

<details>
<summary><b>Debian / Ubuntu / Linux Mint</b></summary>

```console
sudo apt install libgtk-4-dev libadwaita-1-dev pkg-config build-essential \
    gcc make dkms curl tar linux-headers-$(uname -r)
```
</details>

<details>
<summary><b>Fedora</b></summary>

```console
sudo dnf install gtk4-devel libadwaita-devel pkg-config gcc make \
    dkms curl tar kernel-devel-$(uname -r)
```
</details>

<details>
<summary><b>Arch Linux</b></summary>

```console
sudo pacman -S gtk4 libadwaita pkgconf gcc make dkms curl tar linux-headers
```
</details>

**Rust** (kurulu değilse):
```console
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### Derleme & Kurulum

```console
# Depoyu klonlayın
git clone https://github.com/cleyton1986/predator-sense.git
cd predator-sense/predator-sense-gui

# GUI'yi ve Rust yükleyici/servislerini derleyin
cargo build --release
cargo build --release --manifest-path installer/Cargo.toml

# Yerel derlemeyi kurun ve mevcut C çekirdek kaynaklarını DKMS'e kaydedin
sudo installer/target/release/predator-sense-installer --install

# Çalıştırın
/opt/predator-sense/predator-sense
```

---

## Kullanım

### Klavye RGB

1. Kenar çubuğundan **Aydınlatma**'ya gidin
2. **Statik** (bölge başına renkler) veya **Dinamik** (efektler) seçin
3. **Statik mod:** klavyenin 4 bölümünün her biri için R/G/B kaydırıcılarını ayarlayın
4. **Dinamik mod:** bir efekt seçin (Breathing, Neon, Wave, Shifting, Zoom) ve hızı ayarlayın
5. **Uygula**'ya tıklayın

> Çekirdek modülü olmayan, yalnızca I2C-HID kullanan donanımlarda (bkz. [Uyumluluk](#uyumluluk)), Breathing ve Neon gerçekten animasyon yapar; Wave/Shifting/Zoom yalnızca ekranda önizleme gösterir ve açıkça öyle etiketlenir — fiziksel klavye bunlar için henüz değişmez.

### RGB Kapak Logosu

1. **Aydınlatma**'ya gidin ve **Kapak logosu**'nu seçin (seçici yalnızca uyumlu bir HID hedefi algılandığında görünür)
2. Amblemi açmak veya kapatmak için **Aydınlatma**'yı kullanın
3. **Statik**, **Breathing** veya **Neon** seçin, ardından mevcut renk, parlaklık ve hız kontrollerini canlı önizlemeye bakarak ayarlayın
4. **Logoya uygula**'ya tıklayın

Son başarıyla uygulanan durum, kullanıcı hotkey servisi başladığında ve uyku/hazırda beklemeden sonra geri yüklenir. Animasyonlu efekt renkleri firmware tarafından kontrol edilir, bu yüzden önizleme bir renk seçici sunmak yerine bu modların davranışını temsil eder.

> Linux kullanıcı servisini başlatmadan önce gösterilen aydınlatma animasyonunun sahibi firmware'dir. Kaydedilmiş bir "kapalı" durum girişten sonra geri yüklenir, ancak bu uygulama önceki BIOS/açılış animasyonunu bastıramaz.

### Performans Profilleri

Intel P-State + HWP'nin aktif olduğu sistemlerde CPU tarafı şu şekilde çözümlenir:

| Profil | HWP politikası | Intel EPP | Min. performans | GPU Gücü | Fan | Kullanım Amacı |
|---------|------------|-----------|------------------|-----------|-----|----------|
| **Eco**⁴ | powersave | power | %5 | 25W³ | Auto | Maksimum pil ömrü |
| **Sessiz** | powersave | power | %10 | 40W³ | Auto | Sessiz çalışma |
| **Dengeli** | powersave | balance_performance | %17 | 80W³ | Auto | Genel kullanım |
| **Performans** | powersave¹ | performance | %50 | 100W³ | Max | Oyun |
| **Turbo** | performance² | 0 (çekirdek tarafından zorlanır) | %100 | 110W³ | Max | Maksimum performans |

Herhangi bir profili seçmek, kendi fan modunu da uygular — ayrıca bir adıma gerek yoktur.
Performans veya Turbo seçmek fanı Max'a iter (fiziksel Turbo tuşuyla aynı);
Sessiz, Dengeli ve Eco onu Auto'da bırakır.

⁴ Yalnızca pilde, resmi Windows uygulamasıyla aynı şekilde: Eco'yu şarjda hiçbir zaman
seçenek olarak sunmaz, bu yüzden bu kart yalnızca fişten çekiliyken Mod sayfasında görünür.
Bu katman için doğrulanmış bir Acer watt/EPP değeri yoktur, bu yüzden ayarları diğer
dört katmanın aksine ölçülmüş bir değer değil, Sessiz'in kendi değerlerinin altında
tutucu bir dış çıkarımdır.

¹ Intel P-State'in HWP `powersave` politikası, genel minimum-frekans governor'ı değil,
dinamik bir ölçekleme algoritmasıdır. Modele özgü adlandırılmış EPP'yi yazılabilir tutar,
bu da Performans'ı %50'den maksimuma dinamik bir katman yapar.

² HWP `performance` politikasının kendisi EPP'yi 0'a zorlar ve kullanılabilir P-state
aralığını üst sınırına kısıtlar. Predator Sense, sayısal EPP yazımları gerektirmek yerine
bu çekirdek davranışına dayanır. Arka uç, bir CPU model izin listesi olmadan her
cpufreq politikasından algılanır. Diğer sürücüler mevcut `performance` + adlandırılmış
`performance` eşlemesini korur, EPP'si olmayan sistemler yalnızca bu isteğe bağlı
kontrolü atlar.

³ `nvidia-smi -pl` üzerinden en iyi çaba ile, aşağıdaki GPU Paneli'nin güç limiti
kaydırıcısıyla aynı şekilde - `nvidia-smi` mevcut değilse sessizce atlanır ve bazı
dizüstülerde vBIOS, NVML'in güç limiti kontrolünü hiç sunmaz (`nvidia-smi -q`
`Power Management Object: N/A` bildirir, istenen değer ne olursa olsun her `-pl`
değeri reddedilir). Bu, bu uygulamanın - ya da herhangi bir Linux yazılımının -
değiştirebileceği bir şey değil, firmware seviyesinde bir kısıtlama; bunu yükseltmek
`nvflash` gibi yalnızca Windows'ta çalışan bir araçla farklı bir vBIOS flaşlamak
anlamına gelir, GPU'yu tuğlaya çevirme riski gerçektir ve tamamen sahibinin kararıdır.

**Resmi Windows uygulamasından bilinen fark:** Sessiz modda, resmi PredatorSense
ayrıca NVIDIA'nın Whisper Mode'unu da açar (`NvAPI_NvToppsJpacSetControl`),
bu da fan eğrisinin daha sessiz çalışmasına izin vermek için kare hızını 60 FPS'de
sınırlar. Bu kontrol, NVIDIA'nın yalnızca Windows'ta çalışan sürücü API'sinin bir
parçasıdır ve Linux karşılığı yoktur, bu yüzden buradaki Sessiz, aynı donanımda
Windows'taki Sessiz kadar sessiz değildir yük altında - bu bir platform kısıtlamasıdır,
bu uygulamadaki bir hata değil.

### Firmware güç profilleri (ölçüldü, tahmin edilmedi)

Yukarıdaki tablodaki her şey, yalnızca mevcut bir güç bütçesini CPU ve GPU arasında
yeniden dağıtır. **Paket güç limitinin kendisi** firmware'in kendi termal profili
tarafından belirlenir ve bazı modellerde firmware en düşük profiliyle açılır — bu
yüzden hiçbir governor, EPP veya `min_perf` değişikliği tavanı bir watt bile yükseltmez.

`platform_profile` her zaman bu modlara ulaşamaz. Çekirdek sürücüsü bunları sabit bir
tablodan adlandırır (`BALANCED=0, QUIET=1, PERFORMANCE=2, TURBO=3, ECO=4`) ve bu her
firmware'de geçerli değildir. Bir Predator PHN16-73'te (Arrow Lake, BIOS V1.26)
ölçülmüştür, her ham indeksi yazıp paket limitini geri okuyarak:

| Firmware indeksi | Sürekli (PL1) | Ani (PL2) | `platform_profile` üzerinden adı |
|---:|---:|---:|---|
| 6 | 45 W | 50 W | *(yok — ulaşılamaz)* |
| 0 | 55 W | 160 W | `balanced` |
| 1 | 70 W | 160 W | `quiet` |
| 4 | 95 W | 160 W | `low-power` |
| 5 | **115 W** | 160 W | *(yok — ulaşılamaz)* |

En güçlü ve en zayıf modların hiçbir adı yok ve adı olan üçü de yanlış sırada
etiketlenmiş. Düzeltilmiş bir tabloyu sabit kodlamak sorunu sadece bir sonraki
firmware'e taşır, bu yüzden Predator Sense bunun yerine **ölçer**:

1. Çekirdek modülü, ham indeksi ve firmware'in kendi desteklenen-indeks bit maskesini
   `/sys/devices/platform/acer-wmi/thermal_profile` ve `thermal_profile_supported`
   olarak sunar.
2. **Mod → Profilleri kalibre et**, desteklenen her indeksi yazar ve ortaya çıkan
   paket limitini `intel-rapl-mmio`'dan okur, ardından bunları sürekli güce göre
   sıralar. Birkaç saniye sürer ve çalışırken fanları duyulur şekilde hareket ettirir.
3. O andan itibaren yukarıdaki dört katman da firmware profilini yönlendirir, Sessiz'in
   gerçek en zayıf, Turbo'nun gerçek en güçlü profile denk gelmesi için sabitlenmiş
   olarak.

Notlar:

- **Okunabilir RAPL'i olmayan makineler** (AMD modelleri, daha eski Intel'ler)
  sıralanamaz. Profiller yine de listelenir ve elle değiştirilebilir, ancak dört
  katman, bir sıra tahmin etmek yerine bilinçli olarak firmware'i kendi haline
  bırakır — yukarıdaki firmware'de, indekse göre tahmin etmek Turbo'yu 45 W'lık
  profile koyardı.
- Firmware, profili her güç döngüsünde **unutur**, bu yüzden önyükleme servisi
  seçtiğiniz son profili yeniden uygular.
- Firmware'in klavye aydınlatmasını güç moduna bağladığı modellerde, her geçiş —
  bir kalibrasyonun her adımı dahil — klavyeyi yeniden boyar. Bunu yapan
  firmware'dir, bu uygulama değil; sizi rahatsız ediyorsa renklerinizi Aydınlatma
  sayfasından yeniden uygulayın.
- Fiziksel **mod değiştirme tuşu** aynı ölçülmüş sırada döner; aşağıya bakın.

### Fiziksel mod değiştirme tuşu

Bazı modellerde güç modlarını sırayla değiştiren özel bir tuş bulunur. Bu tuş
**yalnızca** gömülü denetleyici üzerinde ham bir HID giriş raporu olarak bildirilir
ve hiçbir input-subsystem olayı üretmez, bu yüzden PredatorSense tuşu (bir WMI
hotkey'i) çalışırken bu tuş Linux'ta ölü gibi görünür.

Daemon, bunun için Acer EC HID cihazını izler. Varsayılanlar bir PHN16-73'te
yakalandı (`1025:174B`, rapor `04 85 ff`); diğer modellerin farklı olması beklenir,
bu yüzden ikisi de yeniden derlemeye gerek kalmadan geçersiz kılınabilir:

`~/.config/predator-sense/mode_key.json`:

```json
{ "product": "0000ABCD", "report": [4, 133, 255] }
```

(katı JSON — bu dosyada bir `//` yorumu onu ayrıştırılamaz hale getirir ve daemon,
günlüğüne bir not düşerek varsayılanlara döner.)

Tuşunuz hiçbir şey yapmıyorsa, daemon başlangıçta bulduğu her Acer HID cihazını
günlüğe kaydeder (Ayarlar'da `debug_logging`'i etkinleştirin). Tuşa basarken
`sudo hexdump -C /dev/hidrawN` ile doğru olanı bulun, sonra dosyayı ona
yönlendirin — ve lütfen değerlerle birlikte bir issue açın ki modeliniz için
varsayılan olarak gönderilebilsinler.

Firmware ayrıca pil %40'ın altındayken mod değiştirmeyi reddeder; daemon, tuşun
bozuk görünmesine izin vermek yerine bunu bildirir.

### Enerji kaynağına göre otomatik profil

Ayarlar'da etkinleştirildiğinde (yeni kurulumlarda varsayılan olarak açık), bu
yalnızca fişi takıp çıkarmaya bir tepki değildir - sürekli olarak uygulanır:
- **Şarjda:** her zaman Performans veya Turbo. Bu ikisinden biri zaten aktifse,
  olduğu gibi bırakılır - otomatik değiştirici, ikisi arasında manuel bir seçimle
  hiçbir zaman çelişmez.
- **Pilde:** her zaman Dengeli veya Sessiz, asla Performans/Turbo değil. Pil
  %15'in altındayken, yapılandırılan hedef ne olursa olsun Sessiz zorlanır.

### GPU Paneli

Gerçek zamanlı NVIDIA GPU izleme:
- Sıcaklık, kullanım, VRAM kullanımı, güç tüketimi (dairesel göstergeler)
- Canlı sıcaklık ve kullanım geçmiş grafikleri (2 dakikalık pencere)
- Çekirdek saat hızı, bellek saat hızı, P-State, PCIe bağlantı bilgisi, VBIOS sürümü

### Yapay Zeka Asistanı (beta)

[Ollama](https://ollama.com) tarafından desteklenen, tamamen kendi makinenizde çalışan, isteğe bağlı yerel bir yapay zeka asistanı — hiçbir şey herhangi bir yere gönderilmez.

1. Ollama'yı ayrıca [resmi Linux talimatlarını](https://ollama.com/download/linux) izleyerek kurun
2. Kenar çubuğundan **AI**'ya gidin ve yerleşik model yöneticisinden bir model indirin (`smollm2:1.7b` veya daha büyüğü — daha küçük modeller tool-calling'i güvenilir şekilde desteklemez)
3. Asistanı **Ayarlar**'da etkinleştirin ve **Otomatik uygula**yı (önerileri hemen uygular) veya **Her zaman onayla**yı (varsayılan — önerilen her değişiklik onayınızı bekler) seçin

Asistan, canlı donanım durumunu (sıcaklık, fan, termal profil, pil) okur ve sabit, önceden doğrulanmış bir eylem kümesi üzerinden değişiklikler önerebilir veya uygulayabilir — hiçbir zaman ham donanıma/EC'ye doğrudan dokunmaz ve her eylem, yapay zeka özelliği var olmadan önce bu uygulamanın zaten kullandığı bir fonksiyonla bire bir eşleşir. Model yalnızca bir analiz çalıştırmak için yüklenir, sonra bellekten kaldırılır — bellekte boşta oturmaz. Tüm yapay zeka etkinliği aynı sayfada kalıcı, incelenebilir bir eylem günlüğüne kaydedilir.

---

## Yükleyici Seçenekleri

Rust yükleyici, etkileşimli bir TUI sunar:

```console
sudo ./predator-sense-installer              # Etkileşimli menü
sudo ./predator-sense-installer --install    # Doğrudan tam kurulum
sudo ./predator-sense-installer --uninstall  # Her şeyi kaldır
sudo ./predator-sense-installer --reload-module # Çekirdek modülünü yeniden derle/yükle
sudo ./predator-sense-installer --status     # Bileşen durumunu göster
```

---

## Kaldırma

```console
sudo ./predator-sense-installer  # 2. seçeneği seçin
```

Ya da elle:
```console
pkill -f "/opt/predator-sense/predator-sense"
sudo rm -rf /opt/predator-sense
sudo rm -f /usr/share/applications/predator-sense.desktop
sudo rm -f /usr/share/icons/hicolor/128x128/apps/predator-sense.png
rm -f ~/.config/systemd/user/predator-sense-hotkey.service
rm -f ~/.config/autostart/predator-sense-hotkey.desktop
sudo rmmod facer  # İsteğe bağlı: çekirdek modülünü kaldır
```

---

## Sorun Giderme

<details>
<summary><b>Klavye RGB değişmiyor / bir efektte takılı kalıyor</b></summary>

Çekirdek modülünün durumu takılmış olabilir. Yeniden yükleyin:
```console
sudo rmmod facer
sudo insmod /path/to/kernel/facer.ko
# Ya da yükleyiciyi kullanın: sudo ./predator-sense-installer → 4. seçenek
```
</details>

<details>
<summary><b>Modül yüklenmiyor</b></summary>

```console
# WMI cihazının var olduğunu kontrol edin
ls /sys/bus/wmi/devices/7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56/

# Çekirdek günlüklerini kontrol edin
sudo dmesg | grep -i facer

# Başlıkların çekirdeğinizle eşleştiğinden emin olun
sudo apt install linux-headers-$(uname -r)
```
</details>

<details>
<summary><b>PredatorSense tuşu çalışmıyor</b></summary>

```console
# Rust hotkey servisini kontrol edin
systemctl --user status predator-sense-hotkey.service
pgrep -af predator-sense-hotkey

# Kullanıcının 'input' grubunda olduğundan emin olun (eklendikten sonra tam çıkış/giriş veya yeniden başlatma gerekir)
groups | grep input
sudo usermod -aG input $USER
```
</details>

<details>
<summary><b>NVIDIA GPU sayfası veri göstermiyor</b></summary>

```console
# nvidia-smi'nin çalıştığını doğrulayın
nvidia-smi
# Çalışmıyorsa, NVIDIA özel sürücülerini kurun
```
</details>

<details>
<summary><b>Modelime uygun bir quirk yok (eksik profiller/fan okuma/PWM)</b></summary>

Tam modeliniz uyumluluk listesinde henüz yoksa, isteğe bağlı her `predator_v4` ailesi özelliğini zorla açıp donanımınızda gerçekte nelerin çalıştığını görmeyi deneyin:

```console
sudo modprobe facer enable_all=1
# yeniden başlatmalar arasında kalıcı:
echo "options facer enable_all=1" | sudo tee /etc/modprobe.d/facer-options.conf
```

Bu yalnızca WMI kullanır (ham EC yazımı yoktur), bu yüzden bir özelliği uygulamayan donanımlarda güvenli bir no-op'tur, kötü bir yazım değil. Lütfen modelinizle ve neyin çalışıp çalışmadığıyla birlikte [bir issue açın](https://github.com/cleyton1986/predator-sense/issues) — yeni quirk'ler böyle eklenir.
</details>

---

## Proje Yapısı

```
predator-sense-gui/
├── kernel/                      # Linux çekirdek modülleri (DKMS ile yönetilir)
│   ├── facer.c                  # Acer donanımına ACPI/WMI arayüzü
│   ├── acer-wmi-battery.c       # Pil şarj limiti desteği
│   ├── acpi_ec.c                # /dev/ec üzerinden ham EC erişimi (MusiKid/acpi_ec'den)
│   ├── Makefile
│   └── dkms.conf                # DKMS otomatik yeniden derleme yapılandırması
├── installer/                   # Rust multicall yükleyici ve servisler
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # Kurulu çalıştırılabilir dosya adına göre tipli dağıtım
│       ├── constants.rs         # Merkezi yollar, protokol değerleri ve donanım sabitleri
│       ├── install.rs           # Yükleyici + DKMS kaydı
│       ├── helper.rs            # Doğrulanmış ayrıcalıklı donanım işlemleri
│       ├── hotkey.rs            # Linux input-event dinleyicisi
│       ├── tray.rs              # StatusNotifierItem servisi
│       └── i18n.rs              # Tipli EN/PT mesajları
├── protocol/                    # Paylaşılan tipli GUI/helper sözleşmesi
│   ├── Cargo.toml
│   └── src/lib.rs               # Eylemler, yollar, limitler ve binary adları
├── src/                         # Rust GTK4 uygulaması
│   ├── main.rs
│   ├── app_state.rs             # Global pencere-görünürlüğü bayrağı (zamanlayıcıları kapatır)
│   ├── i18n.rs                  # EN/PT uluslararasılaştırma
│   ├── config.rs                # Kullanıcı tercihleri (JSON)
│   ├── tray.rs                  # Rust tray-servisi yaşam döngüsü
│   ├── hardware/
│   │   ├── helper.rs            # Tipli ayrıcalıklı-yardımcı istemcisi
│   │   ├── rgb.rs               # /dev/acer-gkbbl-* üzerinden RGB
│   │   ├── hwmon.rs             # /sys/class/hwmon indeksi (önbelleğe alınmış OnceLock)
│   │   ├── sensors.rs           # Sıcaklıklar, fanlar, RAM, ağ
│   │   ├── gpu.rs               # TTL önbellekli nvidia-smi ayrıştırıcı
│   │   ├── procs.rs             # /proc örnekleyici (çekirdek başına CPU, bellek, süreç listesi)
│   │   ├── storage.rs           # df üzerinden disk kullanımı
│   │   ├── sysinfo.rs           # DMI + CPU + GPU + işletim sistemi özellikleri
│   │   ├── fan.rs               # Fan modu + CoolBoost
│   │   ├── extras.rs            # Pil limiti, LCD overdrive, USB şarj, açılış animasyonu
│   │   ├── profile.rs           # CPU governor + EPP + GPU gücü
│   │   ├── ai_assistant.rs      # Ollama tool-calling: mevcut hardware:: setter'larına eşlenen sabit izin listesi
│   │   ├── ai_snapshot.rs       # Yapay zekaya beslenen, her okumadan sonra temizlenen geçici donanım durumu anlık görüntüsü
│   │   ├── ai_actionlog.rs      # Yapay zekanın önerdiği/uyguladığı her şeyin kalıcı, incelenebilir günlüğü
│   │   └── setup.rs             # Çekirdek modülü yönetimi
│   └── ui/                      # GTK4 sayfaları (Cairo özel widget'ları)
│       ├── window.rs            # Ana pencere, kenar çubuğu, neon çubuklar, tepsiye gizleme
│       ├── dashboard_page.rs    # Ana görsel + sistem özellikleri
│       ├── temperatures_page.rs # Tüm sıcaklık göstergeleri
│       ├── usage_page.rs        # En çok kaynak tüketen süreçlerle CPU/GPU/Bellek/Depolama
│       ├── network_page.rs      # Zirve takibiyle indirme/yükleme
│       ├── rgb_page.rs          # Görsel bölgeli klavye RGB'si
│       ├── fan_control_page.rs  # Animasyonlu fanlar + CoolBoost
│       ├── fan_page.rs          # Performans profilleri
│       ├── battery_page.rs      # Pil istatistikleri + şarj limiti
│       ├── gpu_page.rs          # NVIDIA GPU paneli
│       ├── monitor_page.rs      # Detaylı CPU/GPU geçmiş grafikleri
│       ├── ai_page.rs           # Yapay Zeka Asistanı: sohbet, model yöneticisi, kaynak izleyicisi, eylem günlüğü
│       ├── setup_page.rs        # Çekirdek modülü kurulum sihirbazı
│       └── gauge_widget.rs      # Kesikli dairesel gösterge widget'ı
└── resources/
    ├── style.css                # Oyuncu koyu teması
    └── predator-icon.svg        # Sistem tepsisi simgesi
```

---

## Katkı ve Teşekkürler

- **`facer` çekirdek modülü**, [JafarAkhondali](https://github.com/JafarAkhondali) ve [tüm katkıda bulunanların](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module/graphs/contributors) [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) projesine dayanır
- **`acpi_ec` çekirdek modülü**, [Sayafdine Said (MusiKid)](https://github.com/MusiKid/acpi_ec) tarafından — ham EC okuma/yazma için `/dev/ec`'yi sunar. Fan modlarını, CoolBoost'u, LCD overdrive'ı, USB şarjı ve açılış animasyonunu ayarlamak için yardımcı tarafından kullanılır.
- **GUI Uygulaması**, [Rust](https://www.rust-lang.org/) + [GTK4](https://gtk.org/) + [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/) ile geliştirildi
- **Yükleyici ve arka plan servisleri**, [Rust](https://www.rust-lang.org/) ile geliştirildi; tray entegrasyonu [ksni](https://crates.io/crates/ksni) kullanır
- **Dashboard ve Sıcaklıklar simgeleri** (`predator-sense-gui/resources/icons/`), [Flaticon](https://www.flaticon.com)'dan, Hilmy Abiyyu A., magnific ve mehwish tarafından oluşturuldu

### Bu projeyi fork'lamak veya yeniden kullanmak

Bu proje GPL-3.0 altında lisanslanmıştır, bu yüzden fork'lamakta, değiştirmekte ve aynı lisans altında yeniden dağıtmakta özgürsünüz. Bunu yaparsanız — özellikle türev bir uygulama geliştiriyor veya GUI/çekirdek modülünün önemli bir kısmını yeniden kullanıyorsanız — **lütfen orijinal yazara görünür bir atıf bırakın** (README'nizde, Hakkında ekranınızda veya credits bölümünüzde [Cleyton Alves](https://github.com/cleyton1986)'ten / bu depodan bir bahis yeterlidir). Bağımsız, ücretsiz bir yan proje için küçük bir istek, ama çok büyük fark yaratıyor.

## Projeyi Destekleyin

Bu proje işinize yaradıysa ve gelişimini desteklemek isterseniz, bana bir kahve ısmarlamayı düşünebilirsiniz:

<p align="center">
  <a href="https://www.paypal.com/cgi-bin/webscr?cmd=_donations&business=cleyton1986%40gmail.com&currency_code=BRL&item_name=Predator+Sense+for+Linux">
    <img src="https://img.shields.io/badge/PayPal-Donate-00457C?logo=paypal&logoColor=white&style=for-the-badge" alt="Donate via PayPal">
  </a>
</p>

<p align="center">
  <b>PIX (Brezilya):</b> <code>cleyton1986@gmail.com</code>
</p>

Her katkı gönüllüdür ve büyük memnuniyetle karşılanır! Projenin canlı kalmasına yardımcı olur ve yeni özellikler için motive eder.

---

## Lisans

Bu proje **GNU General Public License v3.0** altında lisanslanmıştır — ayrıntılar için [LICENSE](LICENSE) dosyasına bakın.

Bu özgür bir yazılımdır: Özgür Yazılım Vakfı tarafından yayımlanan GNU GPL şartları altında yeniden dağıtabilir ve/veya değiştirebilirsiniz.

**İstisna — ürün görselleri:** yukarıdaki GPLv3 lisansı yalnızca bu projenin kaynak kodunu kapsar. `predator-sense-gui/resources/models/` altındaki Acer Predator/Nitro dizüstü fotoğrafları üçüncü taraf ürün görselleridir (yukarıdaki [Yasal Uyarı](#yasal-uyarı) bölümüne bakın) ve GPLv3 lisansı kapsamında **değildir**; bu görsellerdeki tüm haklar Acer Inc.'e ve/veya orijinal fotoğrafçılara aittir.

**Bu yazılım, hiçbir garanti olmaksızın "olduğu gibi" sağlanmaktadır.** Yazarlar, bu yazılımın kullanımından kaynaklanabilecek herhangi bir zarardan sorumlu değildir. Bu yazılımı kurarak ve kullanarak, bunu tamamen kendi sorumluluğunuzda yaptığınızı kabul etmiş olursunuz.
</content>
