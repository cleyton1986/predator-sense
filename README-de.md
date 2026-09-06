# Predator Sense für Linux

<p align="center">
  <a href="README.md">🇺🇸 Read in English</a> · <a href="README-ptbr.md">🇧🇷 Leia em Português</a> · <a href="README-es.md">🇪🇸 Leer en Español</a> · <a href="README-zh.md">🇨🇳 阅读中文版</a> · <a href="README-ja.md">🇯🇵 日本語で読む</a> · <a href="README-ru.md">🇷🇺 Читать на русском</a> · <a href="README-it.md">🇮🇹 Leggi in Italiano</a> · <a href="README-tr.md">🇹🇷 Türkçe Oku</a>
</p>

<p align="center">
  <img src="predator-sense-gui/resources/logo.jpeg" width="120" alt="Predator Sense Logo">
</p>

<p align="center">
  <b>Inoffizielles Linux-Kernelmodul und GUI zur Hardwaresteuerung von Acer-Gaming-Notebooks</b><br>
  <i>RGB-Tastaturbeleuchtung &bull; Turbo-Modus &bull; Temperaturüberwachung &bull; Leistungsprofile</i>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Sprache-Rust-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/GTK-4-blue?logo=gtk" alt="GTK4">
  <img src="https://img.shields.io/badge/Userspace-100%25_Rust-orange?logo=rust" alt="100% Rust userspace">
  <img src="https://img.shields.io/badge/Lizenz-GPL--3.0-green" alt="License">
  <img src="https://img.shields.io/badge/Plattform-Linux-yellow?logo=linux" alt="Linux">
</p>

<p align="center">
  Erstellt und gepflegt von <a href="https://github.com/cleyton1986">Cleyton Alves</a>
</p>

---

## Haftungsausschluss

> **Warnung**
> **Nutzung auf eigenes Risiko!** Dies ist ein **inoffizielles** Projekt. Acer war an der Entwicklung nicht beteiligt. Das Kernelmodul wurde durch Reverse Engineering der offiziellen PredatorSense-Windows-Anwendung entwickelt. Dieser Treiber verwendet systemnahe WMI-/ACPI-Methoden, die nicht auf allen Notebook-Serien getestet wurden. Die Autoren übernehmen keine Verantwortung für Schäden an Ihrer Hardware.

> **Hinweis**
> Alle genannten Marken, Produktnamen und Logos (Acer, Predator, PredatorSense, Helios, Nitro, AeroBlade, CoolBoost) sind Eigentum ihrer jeweiligen Inhaber (Acer Inc.). Dieses Projekt steht in keiner Weise mit Acer Inc. in Verbindung und wird von Acer Inc. weder unterstützt noch gesponsert.

> **Produktbilder**
> Die Notebook-Fotos unter `predator-sense-gui/resources/models/` zeigen offizielle Acer-Predator-/Nitro-Produkte und dienen ausschließlich dazu, der App die visuelle Identifikation des auf dem eigenen Gerät des Nutzers erkannten Modells zu ermöglichen (abgeglichen mit dem `product_name`, den DMI/BIOS des Systems melden). Diese Bilder sind **nicht durch die GPLv3-Lizenz dieses Projekts abgedeckt**: Das Copyright an den zugrunde liegenden Produktfotos liegt bei Acer Inc. und/oder deren ursprünglichen Urhebern. Sie werden hier in gutem Glauben, auf nicht kommerzieller, rein informativer Grundlage (nominative Verwendung zur Produktidentifikation) bereitgestellt, ohne dass dieses Projekt einen Eigentumsanspruch erhebt. Falls Sie Rechteinhaber sind und die Entfernung eines Bildes wünschen, eröffnen Sie bitte ein Issue, es wird umgehend entfernt.

Diese Anwendung wurde für den **persönlichen Gebrauch** entwickelt, um unter Linux das Beste aus einem Acer-Gaming-Notebook herauszuholen, da Acer für PredatorSense keinen offiziellen Linux-Support anbietet. Sie wird frei mit allen geteilt, die dasselbe möchten.

Wenn diese App/dieses Projekt Ihnen geholfen hat und/oder Ihnen auf irgendeine Weise gefallen hat, denken Sie bitte darüber nach, einen Stern zu hinterlassen, das hilft sehr ⭐

---

## Screenshots

<p align="center"><b>Dashboard</b>: Notebook-Foto und vollständige Systemspezifikationen auf einen Blick: CPU, GPU, RAM, Speicher, Netzwerk und Betriebssystem.</p>
<p align="center"><img src="assets/psense-1.png" width="800" alt="Dashboard"></p>

<p align="center"><b>Temperaturen</b>: Live-Anzeigen für CPU, GPU, System, NVMe-Laufwerke, WLAN und RAM, alles auf einem Bildschirm.</p>
<p align="center"><img src="assets/psense-2.png" width="800" alt="Temperatures"></p>

<p align="center"><b>Auslastung</b>: CPU, GPU, Speicher und Datenträger mit den ressourcenintensivsten Prozessen, animierten Balken und ausklappbaren Details per Klick (mit einer CSS-basierten Feueranimation an der Temperaturanzeige).</p>
<p align="center"><img src="assets/psense-3.png" width="800" alt="Usage"></p>

<p align="center"><b>Netzwerk</b>: Echtzeit-Diagramme für Download/Upload mit Spitzenwertverfolgung und automatischer Schnittstellenerkennung (WLAN oder Ethernet).</p>
<p align="center"><img src="assets/psense-4.png" width="800" alt="Network"></p>

<p align="center"><b>Beleuchtung</b>: Statische Farben pro Zone (4 Bereiche) und dynamische RGB-Tastatureffekte (Breathing, Neon, Wave, Shifting, Zoom).</p>
<p align="center"><img src="assets/psense-5.png" width="800" alt="Lighting"></p>

<p align="center"><b>Modi</b>: Leistungsprofile: Quiet, Balanced, Performance und Turbo, dazu eine nur im Akkubetrieb verfügbare Eco-Stufe (CPU-Governor + Intel EPP + GPU-Leistungslimit).</p>
<p align="center"><img src="assets/psense-6.png" width="800" alt="Modes"></p>

<p align="center"><b>GameSync</b>: Registrieren Sie ein Spiel und sein Profil; die App wechselt automatisch dazu, während das Spiel läuft, und stellt beim Beenden wieder her, was zuvor aktiv war.</p>
<p align="center"><img src="assets/psense-15.png" width="800" alt="GameSync"></p>

<p align="center"><b>Lüftersteuerung</b>: Live-Drehzahl mit animierten, rotierenden Lüftern, CoolBoost-Schalter und Auto-/Max-Modi.</p>
<p align="center"><img src="assets/psense-7.png" width="800" alt="Fan Control"></p>

<p align="center"><b>Akku</b>: Ladestand in Prozent, Spannung, Stromstärke, Leistung, Ladezyklen, Zustand, Hersteller und 80%-Ladelimit für eine längere Lebensdauer.</p>
<p align="center"><img src="assets/psense-8.png" width="800" alt="Battery"></p>

<p align="center"><b>GPU</b>: NVIDIA-Dashboard mit Live-Diagrammen, Taktraten, Auslastung, VRAM, Leistungsaufnahme und PCIe-Informationen.</p>
<p align="center"><img src="assets/psense-9.png" width="800" alt="GPU"></p>

<p align="center"><b>Diagramme</b>: Detaillierte CPU- und GPU-Verlaufsdiagramme mit Min./Max.-Verfolgung.</p>
<p align="center"><img src="assets/psense-10.png" width="800" alt="Graphs"></p>

<p align="center"><b>KI-Assistent (beta)</b>: Lokaler KI-Assistent auf Basis von Ollama: Chat, Modellverwaltung (installierte Modelle auflisten, neue herunterladen, auswählen, welches ausgeführt wird), Live-VRAM-/GPU-Ressourcennutzung während der Verarbeitung und ein dauerhaftes Aktionsprotokoll.</p>
<p align="center"><img src="assets/psense-11.png" width="800" alt="AI Assistant"></p>

<p align="center"><b>Treiber und Handbücher</b>: Zeigt die Seriennummer (mit Kopierschaltfläche) und einen direkten Link zur offiziellen Treiber-und-Handbücher-Seite von Acer, dazu eine Abbildung, wo sich der Seriennummer-Aufkleber am Notebook befindet.</p>
<p align="center"><img src="assets/psense-16.png" width="800" alt="Drivers and manuals"></p>

<p align="center"><b>Einstellungen</b>: In den Tray minimieren, Start beim Hochfahren, automatisches Anwenden des Profils beim Start, Spracheinstellungen und eine modellspezifische Liste unterstützter Funktionen.</p>
<p align="center"><img src="assets/psense-12.png" width="800" alt="Settings"></p>

<p align="center"><b>Deckel-Logo-Beleuchtung</b>: Unabhängige RGB-Steuerung für das Logo auf der Rückseite des Displays, bei Modellen mit farbfähigem Deckel-Logo (Static/Breathing/Neon). Zur Laufzeit erkannt: Die Steuerung erscheint nur, wenn die Hardware auf eine Funktionsabfrage antwortet, und bleibt bei Modellen ohne diese Funktion sicher ausgeblendet.</p>
<p align="center"><img src="assets/psense-13.png" width="800" alt="Cover logo lighting"></p>
<p align="center"><img src="assets/psense-14.jpg" width="800" alt="Cover logo lit up green on a Predator PHN16-73"></p>
<p align="center"><sub>Funktion beigetragen von <a href="https://github.com/jlucaso1">@jlucaso1</a>, getestet am eigenen Predator PHN16-73. Das Deckel-Logo dieses Notebooks ist nicht farbfähig, weshalb die Funktion mit dessen Hardware verifiziert wurde.</sub></p>

---

## Über

Inoffizielles Linux-Kernelmodul für die RGB-Tastaturbeleuchtung und den Turbo-Modus von Acer-Gaming-Notebooks (Acer Predator, Acer Helios, Acer Nitro).

Inspiriert von und aufbauend auf dem Projekt [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) von [JafarAkhondali](https://github.com/JafarAkhondali) und Mitwirkenden. Dieses Projekt erweitert das bestehende Linux-Acer-WMI-Kernelmodul um Acer-Gaming-Funktionen und fügt eine **vollständige grafische Desktop-Anwendung** hinzu, entwickelt mit Rust und GTK4.

---

## Funktionen

| Funktion | Beschreibung |
|---------|-------------|
| **Dashboard** | Notebook-Foto + vollständige Systemspezifikationen (CPU, GPU, RAM, Speicher, Netzwerk, Betriebssystem) |
| **Temperaturen** | Live-Anzeigen für CPU, GPU, System, NVMe, WLAN und RAM |
| **Auslastung** | Ansicht mit 4 Tabs: CPU / GPU / Speicher / Datenträger mit den ressourcenintensivsten Prozessen, ausklappbaren Details per Klick und CSS-basierter Feueranimation an den Temperaturanzeigen |
| **Netzwerk** | Echtzeit-Diagramme für Download/Upload mit Spitzenwertverfolgung und automatischer Schnittstellenerkennung |
| **RGB-Tastatursteuerung** | Statische Farben pro Zone (4 Zonen) und dynamische Effekte (Breathing, Neon, Wave, Shifting, Zoom) über WMI. Auf Hardware ohne Kernelmodul funktioniert RGB stattdessen nativ über USB/I2C-HID: ENEK5130-Chip (statisch mit 4 Zonen, Breathing/Neon), 2024er-und-neuer-Sunrex-Chip (eine Zone, vollständige Effektliste) oder Chicony-Chip (7-Farben-Palette, Helios 300), automatisch erkannt, siehe [Kompatibilität](#kompatibilität) |
| **RGB-Deckel-Logo** | Unabhängige Steuerung von Ein/Aus, Vollfarbe, Helligkeit, Breathing und Neon für das Emblem auf der Displayrückseite, mit einer Live-Vektorvorschau. Wird erst nach der Erkennung der HID-Funktionalität zur Laufzeit angezeigt |
| **Leistungsprofile** | Modi Quiet / Balanced / Performance / Turbo, dazu eine nur im Akkubetrieb verfügbare Eco-Stufe (CPU-Governor + Intel EPP + GPU-Leistungslimit) |
| **Lüftersteuerung** | Live-Drehzahl mit animierten, rotierenden Lüftern, CoolBoost-Schalter, Auto-/Max-Modi, dazu experimentelle Steuerung der PWM pro Lüfter und automatische Temperaturkurve (falls unterstützt) |
| **Akku** | Ladestatistiken, Ladezyklen, Zustand, Herstellerinformationen und 80%-Ladelimit für eine längere Lebensdauer |
| **GPU-Dashboard** | NVIDIA-Metriken: Temperatur, Auslastung, VRAM, Taktraten, Leistungsaufnahme, PCIe-Informationen mit Live-Diagrammen, dazu ein **Schieberegler für das Leistungslimit (TGP)** |
| **Diagramme** | Detaillierte CPU- und GPU-Verlaufsdiagramme mit Min./Max.-Verfolgung |
| **KI-Assistent** 🧪 | Lokaler, optionaler KI-Assistent auf Basis von [Ollama](https://ollama.com): liest den aktuellen Hardwarezustand und schlägt Änderungen über eine feste, bereits validierte Menge an Aktionen vor oder wendet sie an (Thermalprofil, Lüftermodus, CoolBoost, RGB, GPU-Leistungslimit, Akku). Chat, Modellverwaltung (Herunterladen/Auswählen), Live-Ressourcen-/VRAM-Monitor und ein dauerhaftes Aktionsprotokoll. Automatisches Anwenden oder stets Bestätigung erforderlich, ganz nach Wahl. Erfordert eine separat installierte Ollama-Instanz, siehe [KI-Assistent](#ki-assistent-beta) weiter unten |
| **Automatische Funktionserkennung** | Erkennt, was jedes Modell unterstützt, und passt die Oberfläche entsprechend an: Nicht unterstützte Funktionen werden als „auf diesem Modell nicht verfügbar" angezeigt, statt einen Fehler auszulösen. Unterstützte Funktionen werden in den Einstellungen aufgelistet |
| **Temperaturwarnungen** | Desktop-Benachrichtigung, wenn CPU/GPU 90 °C überschreiten (funktioniert auch aus dem Tray heraus) |
| **Automatisches Energieprofil** | Wechselt das Profil automatisch beim Wechsel zwischen Netz- und Akkubetrieb; das Zielprofil für jeden Zustand ist in den Einstellungen konfigurierbar (Standard: Performance im Netzbetrieb, Balanced im Akkubetrieb) |
| **Debug-Protokollierung** | Optionaler Schalter in den Einstellungen: protokolliert Daemon- und App-Ereignisse nach `~/.local/share/predator-sense/` (rotierend, 5 MB × 3) zur Fehlerdiagnose aus der Ferne. Standardmäßig deaktiviert |
| **System-Tray** | In den Tray minimieren mit dem Predator-Symbol; die App bleibt im Hintergrund aktiv |
| **PredatorSense-Taste** | Zuordnung der Hardware-Taste: die Taste neben NumLock öffnet die App |
| **DKMS** | Kernelmodule werden bei Kernel-Aktualisierungen automatisch neu erstellt |
| **Internationalisierung** | Automatisch Englisch/Portugiesisch je nach Systemsprache |
| **Gaming-Oberfläche** | Dunkles Design mit pulsierenden Neon-Balken, gestrichelten runden Anzeigen, polygonalen Panelrändern. Die Akzentfarbe folgt automatisch der erkannten Marke: Cyan bei Predator/Helios/Triton, Orange/Rot bei Nitro (passend zu NitroSense), keine Einstellung zum Umschalten notwendig |

---

## Kompatibilität

**Funktioniert das auf meinem Notebook?**

Legende: ✅ getestet und funktionsfähig · 🟡 implementiert, nicht getestet (Tester gesucht) · 🧪 experimentell (Tester gesucht) · ❌ funktioniert nicht · `-` nicht zutreffend

| Produktname | Turbo (Impl.) | Turbo (Getestet) | RGB (Impl.) | RGB (Getestet) | Lüfterdrehzahl lesen | Lüfterprofile | Lüfter-PWM % |
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

> Falls Ihr Modell nicht aufgeführt ist, könnte es trotzdem funktionieren: Das Kernelmodul erkennt kompatible WMI-Schnittstellen automatisch. Wenn es bei Ihnen funktioniert hat (oder nicht), eröffnen Sie bitte ein Issue mit Angabe Ihres Modells, damit diese Tabelle aktualisiert werden kann.

### Lüftersteuerung: drei Stufen

| Stufe | Was sie tut | Verfügbarkeit |
|---|---|---|
| **Lüfterdrehzahl lesen** | Liest die Drehzahl von CPU-/GPU-Lüfter (`fan1_input`, `fan2_input`) | Die meisten Gaming-Modelle (automatisch erkannt) |
| **Lüfterprofile** | Quiet / Balanced / Performance / Turbo über `platform_profile` | `predator_v4`-Modelle |
| **Lüfter-PWM %** 🧪 | Drehzahlsteuerung pro Lüfter (`pwm1`/`pwm2` 0-100 %), portiert aus dem Mainline-Treiber `acer-wmi` über WMI, **nur Kernel ≥ 6.14** | Teilmenge der Modelle mit `ACER_CAP_PWM` (AN515-58, PHN16-72/73, PH16-72, …) |

> **🧪 Die PWM-Lüftersteuerung ist experimentell.** Sie wurde aus dem Upstream-Linux-Kernel-Treiber `acer-wmi` portiert und nutzt sichere WMI-Methoden (keine rohen EC-Schreibzugriffe), wurde vom Maintainer aber **nicht auf echter Hardware verifiziert** (dieser besitzt ein PH315-54, das kein PWM hat). Falls Sie ein unterstütztes Modell besitzen, sind Testberichte sehr willkommen. **Nutzung auf eigenes Risiko**, siehe den Haftungsausschluss oben.

### Alternative: linuwu_sense (Hardware ohne passenden Quirk-Eintrag mit nicht funktionierendem Turbo)

Der `enable_all=1`-Fallback von `facer` erkennt jedes WMI-fähige Acer-Board, aber der vollständige `predator_v4`-Profilsatz (5 Profile einschließlich `balanced-performance`/`performance`, beschreibbarer `turbo_state`) gilt nur für Boards, die in seiner DMI-Quirk-Tabelle vorhanden sind. Bei einem Board ohne Quirk-Eintrag ist `platform_profile_choices` auf `low-power quiet balanced` beschränkt, und `turbo_state` bleibt schreibgeschützt, obwohl die Firmware mehr unterstützt; gemeldet für eine PHN16-73-Einheit (Macan_ARX, BIOS V1.26) in [#33](https://github.com/cleyton1986/predator-sense/issues/33).

Falls das Ihr Fall ist: Das Community-Modul [Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense) (geladen mit `predator_v4=1`) stellt den vollständigen Profilsatz über dieselben generischen `platform_profile`/`intel_pstate`/`acer-wmi-battery`-Schnittstellen bereit, die diese App bereits direkt ausliest; kein `facer`-spezifischer Codepfad ist beteiligt. Seit `v0.2.71-preview` erkennt die App `linuwu_sense` und überspringt den Hinweis „facer installieren", wenn dieser Treiber tatsächlich diese Schnittstellen bereitstellt. RGB und die Kalibrierung des Thermalprofils (beide nur mit `facer` verfügbar, siehe oben und unten) benötigen weiterhin `facer` selbst und bleiben unter linuwu_sense nicht verfügbar.

### RGB ohne Kernelmodul (nur I2C-HID-Hardware)

Einige Modelle (bestätigt: PHN16S-71, PHN16-73, AN16S-61) leiten den RGB-Controller der Tastatur über einen separaten I2C-HID-Chip (ENEK5130) statt über die WMI-Schnittstelle von `facer.ko` um: Die App spricht direkt über `/dev/hidrawN` mit ihm, sodass dies auch funktioniert, wenn das Kernelmodul überhaupt nicht geladen ist:

| Funktion | Status |
|---|---|
| Statische Farbe pro Zone, Helligkeit, Hintergrundbeleuchtung aus | ✅ funktioniert bestätigt (PHN16S-71, AN16S-61) |
| Dynamische Effekte, Breathing, Neon | ✅ funktioniert bestätigt (PHN16S-71, AN16S-61): nativ, ein einzelner HID-Schreibvorgang, die Hardware wiederholt das Muster von selbst. Bei der PHN16S-71-Einheit ignoriert Breathing die gewählte Farbe und durchläuft stattdessen einen Regenbogenzyklus; kann bei anderer Hardware abweichen |
| Dynamische Effekte, Wave, Shifting, Zoom | Nur Vorschau am Bildschirm (keine Hardware-Schreibzugriffe): Es hat sich herausgestellt, dass die Effektcodes dafür je nach Hardware-Generation unterschiedliche Bedeutungen haben, daher sind sie noch nicht angebunden |
| RGB-Deckel-Logo, aus, Vollfarbe, Helligkeit, Breathing, Neon | ✅ funktioniert bestätigt (PHN16-73) |

Die Unterstützung des Deckel-Logos wird nicht über eine Zulassungsliste von Modellnamen aktiviert. Der Controller muss das Ziel `0x83` in seinem A1-Zielbericht angeben und passende, nicht leere A3-Fähigkeiten zurückgeben, bevor die Oberfläche angezeigt wird; die App wiederholt diese Prüfung unmittelbar vor jedem Schreibvorgang. Der Hotkey-Daemon stellt nur eine Einstellung wieder her, die die App zuvor nach Anmeldung und Ruhezustand/Standby erfolgreich angewendet hat, und überspringt das Logo vollständig, wenn keine gespeicherte Einstellung vorhanden ist oder das Ziel fehlt.

Ein [unabhängiger Bericht zum AN16S-61](https://github.com/cleyton1986/predator-sense/issues/31) (siehe auch das eigene [eigenständige Protokoll-Tool](https://github.com/ArnarValur/Nitro16S-AI-RGB-Keyboard) des Berichterstatters) hat sechs weitere native Übertragungsmodi jenseits von Static/Breathing/Neon/Wave kartiert (einen Hardware-Aus-Modus, einen Boot-Blink-Modus, den der EC selbst auslöst, sowie vier weitere eingebaute Animationen), dazu ein LED-Ziel für die Modus-/Turbo-Taste. Nichts davon ist bisher in die App eingebunden; dafür wird zunächst ein definierter Slot für hardware-native Effektcodes benötigt, weshalb dies als zukünftige Verbesserung vorgemerkt ist.

Derselbe Bericht enthielt außerdem einen dekodierten HID-Report-Descriptor, der direkt vom Controller ausgelesen wurde, wodurch ein echter Fehler aufgedeckt wurde: Die App las die Zonenanzahl des A3-Fähigkeitsberichts aus dem falschen Byte (`byte[3]`, einer festen Konstante pro Zielklasse) statt aus dem Byte, das der eigene Descriptor des Controllers dafür deklariert (`byte[4]`). Behoben in `v0.2.69-preview`, sowohl in der App als auch im Hotkey-Daemon. Dies ist eine Korrektur auf Protokollebene, keine modellspezifische Änderung: Das Feldlayout des Report-Descriptors stammt aus der eigenen Firmware des Chips (derselbe `0CF2:5130`-Chip in allen drei bestätigten Modellen), und es ändert keine einzige Byte-Übertragung bei bereits als funktionierend bestätigter Hardware, da der vorherige Wert immer eine zu weit gefasste Obermenge des korrekten Werts war.

### RGB auf Hardware ab 2024 (Sunrex/Darfon USB-HID)

Eine neuere Generation (PH16-72 und weitere 2024-2026er-Modelle mit denselben USB-HID-Chips, siehe Issue #26) hat die RGB-Steuerung von Tastatur und Deckel-Logo sowohl von WMI *als auch* vom oben genannten ENEK5130-Chip weg auf ein völlig anderes Controller-Paar verlagert: Sunrex `05af:*` für die Tastatur, Darfon `0d62:*` für das Logo. Die App erkennt und steuert auch diese direkt, automatisch ausgewählt anstelle der ENEK5130-/WMI-Pfade, sobald vorhanden:

| Funktion | Status |
|---|---|
| Tastatur: Off, Static, Breathing, Wave, Snake, Neon, Spot, Star, Rainbow, 5× Slash, Zoom, Row Wave, Swiping | 🟡 implementiert, Bestätigung auf echter Hardware steht aus |
| Deckel-Logo: aus, Vollfarbe, Helligkeit, Breathing | 🟡 implementiert, Bestätigung auf echter Hardware steht aus |

Dieser Chip hat keine unabhängigen Zonen: Die gesamte Tastatur nutzt jeweils eine Farbe/einen Effekt gleichzeitig, anders als der oben genannte 4-Zonen-Controller ENEK5130. Das Übertragungsprotokoll wurde Byte für Byte aus zwei dekompilierten Versionen der offiziellen Windows-Anwendung per Reverse Engineering ermittelt (jede feste Byte-Sequenz und Prüfsummenformel stimmte zwischen beiden exakt überein), nicht geraten; niemand hat es jedoch bisher an physischer Hardware bestätigt, betrachten Sie es daher als ungetestet, bis ein echter Bericht vorliegt.

Ein dritter Chip (Chicony, Helios 300/PH317-56) verwendet ein weiteres USB-HID-Protokoll, dokumentiert durch Reverse Engineering der Community ([NT411/Acer-Predator-Fan-RGB-Controller-Linux](https://github.com/NT411/Acer-Predator-Fan-RGB-Controller-Linux)) und hier anhand dieser Spezifikation neu implementiert: eine feste 7-Farben-Palette (eine Hardware-/Firmware-Beschränkung, kein beliebiges RGB) über 12 Effekte. Ebenfalls 🟡, Bestätigung steht aus.

### Nutzen Sie bereits Linuwu-Sense oder DAMX?

[Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense) (und [DAMX](https://github.com/PXDiv/Div-Acer-Manager-Max), das darauf aufbaut) ist ein separates, unabhängiges Projekt, das ebenfalls Acer-Predator-/Nitro-Hardware unter Linux steuert. Es ist keine Abhängigkeit dieses Projekts, und kein Code davon wird hier verwendet; sein Kernelmodul belegt aber dieselben **WMI-GUIDs**, die `facer` benötigt, und der Kernel erlaubt nicht, dass zwei Treiber gleichzeitig dasselbe Gerät beanspruchen.

Erkennt der Installer, dass `linuwu_sense` bereits geladen oder über DKMS installiert ist, lässt er automatisch **Ihre bestehende Einrichtung unangetastet**: Er setzt `acer_wmi` nicht auf die Sperrliste und erzwingt nicht das Laden von `facer`, sodass er eine bereits funktionierende Linuwu-Sense-/DAMX-Installation nicht stört (oder beschädigt). Die RGB-Tastatursteuerung funktioniert über diese App weiterhin über den HID-Pfad (siehe oben), unabhängig davon, welcher Plattformtreiber aktiv ist; die Lüfter-/Thermalsteuerung verbleibt in diesem Fall bei dem Tool, das Sie bereits dafür verwendet haben.

---

## Installation

### Vorkompilierter Installer (am schnellsten)

Laden Sie den Release-Installer direkt herunter und führen Sie ihn aus:

```console
curl --fail --location https://github.com/cleyton1986/predator-sense/releases/latest/download/predator-sense-installer --output predator-sense-installer
chmod +x predator-sense-installer
sudo ./predator-sense-installer --install
```

Der Installer, der privilegierte Helper, der Hotkey-Listener und der Tray-Dienst werden alle von derselben Rust-Multicall-Binärdatei bereitgestellt. Der Installer lädt alles herunter und konfiguriert es, ohne ein Shellskript-Bootstrapping zu benötigen.

### Interaktiver Installer (vorkompilierte Binärdatei, keine Rust-Toolchain nötig)

Laden Sie die Binärdatei `predator-sense-installer` von der [Releases](../../releases)-Seite herunter. Es handelt sich um eine eigenständige Rust-Binärdatei, kein Bundle: Sie benötigt dennoch Internetzugang, um den Quellcode der App (für das Kernelmodul) und die passende vorkompilierte Release-Binärdatei herunterzuladen, umgeht aber vollständig die Installation von Rust und das Kompilieren der GTK4-App auf Ihrem Rechner:

```console
chmod +x predator-sense-installer
sudo ./predator-sense-installer
```

Wählen Sie **Option 1** (vollständige Installation). Der Installer wird automatisch:

1. Ihre Distribution erkennen (Debian/Ubuntu/Mint, Fedora, Arch)
2. Systemabhängigkeiten installieren (GTK4, libadwaita, Build-Tools, Kernel-Header)
3. Quellcode und vorkompilierte Binärdatei des passenden Releases herunterladen
4. Das Kernelmodul `facer` kompilieren und laden (dieser Teil wird immer lokal kompiliert; Kernelmodule können nicht über verschiedene Kernel-Versionen hinweg vorkompiliert ausgeliefert werden)
5. Einen Eintrag im Anwendungsmenü mit Symbol erstellen
6. Die PredatorSense-Hardware-Taste zuordnen (automatischer Start bei Anmeldung)
7. Die Unterstützung für den System-Tray einrichten

Der vorkompilierte Weg benötigt kein Rust/cargo auf dem Zielrechner. Der Installer wird außerdem nach `/opt/predator-sense/` kopiert, als eigenständiges Verwaltungswerkzeug für Statusprüfungen, das Neuladen des Kernelmoduls, Aktualisierungen und die Deinstallation (siehe [Installer-Optionen](#installer-optionen)).

Nach der Installation öffnen Sie die App wie folgt:
- Drücken der **PredatorSense-Taste** (neben NumLock)
- Suche nach **„Predator Sense"** in Ihrem Anwendungsmenü
- Ausführen von `/opt/predator-sense/predator-sense` in einem Terminal

### Manuelle Installation (aus dem Quellcode kompilieren)

#### Voraussetzungen

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

**Rust** (falls nicht installiert):
```console
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### Kompilieren & Installieren

```console
# Repository klonen
git clone https://github.com/cleyton1986/predator-sense.git
cd predator-sense/predator-sense-gui

# GUI sowie Rust-Installer/Dienste kompilieren
cargo build --release
cargo build --release --manifest-path installer/Cargo.toml

# Den lokalen Build installieren und die vorhandenen C-Kernelquellen bei DKMS registrieren
sudo installer/target/release/predator-sense-installer --install

# Ausführen
/opt/predator-sense/predator-sense
```

---

## Verwendung

### Tastatur-RGB

1. Gehen Sie in der Seitenleiste zu **Beleuchtung**
2. Wählen Sie **Static** (Farben pro Zone) oder **Dynamic** (Effekte)
3. **Static-Modus:** Passen Sie die R/G/B-Regler für jeden der 4 Tastaturbereiche an
4. **Dynamic-Modus:** Wählen Sie einen Effekt (Breathing, Neon, Wave, Shifting, Zoom) und passen Sie die Geschwindigkeit an
5. Klicken Sie auf **Anwenden**

> Auf reiner I2C-HID-Hardware ohne Kernelmodul (siehe [Kompatibilität](#kompatibilität)) animieren Breathing und Neon tatsächlich; Wave/Shifting/Zoom zeigen nur eine Vorschau am Bildschirm, deutlich als solche gekennzeichnet: Die physische Tastatur ändert sich dafür noch nicht.

### RGB-Deckel-Logo

1. Gehen Sie zu **Beleuchtung** und wählen Sie **Deckel-Logo** (die Auswahl erscheint nur, wenn ein kompatibles HID-Ziel erkannt wird)
2. Nutzen Sie **Beleuchtung**, um das Emblem ein- oder auszuschalten
3. Wählen Sie **Static**, **Breathing** oder **Neon** und passen Sie dann die verfügbaren Regler für Farbe, Helligkeit und Geschwindigkeit an, während Sie die Live-Vorschau beobachten
4. Klicken Sie auf **Auf Logo anwenden**

Der zuletzt erfolgreich angewendete Zustand wird beim Start des Benutzer-Hotkey-Dienstes und nach Standby/Ruhezustand wiederhergestellt. Die Farben der animierten Effekte werden von der Firmware gesteuert, daher bildet die Vorschau deren Verhalten ab, statt für diese Modi eine Farbauswahl anzubieten.

> Die Beleuchtungsanimation, die vor dem Start des Linux-Benutzerdienstes angezeigt wird, gehört der Firmware. Ein gespeicherter „Aus"-Zustand wird nach der Anmeldung wiederhergestellt, aber diese App kann die vorherige BIOS-/Boot-Animation nicht unterdrücken.

### Leistungsprofile

Auf Systemen mit aktivem Intel P-State + HWP löst sich die CPU-Seite wie folgt auf:

| Profil | HWP-Richtlinie | Intel EPP | Min. Leistung | GPU-Leistung | Lüfter | Anwendungsfall |
|---------|------------|-----------|------------------|-----------|-----|----------|
| **Eco**⁴ | powersave | power | 5 % | 25 W³ | Auto | Maximale Akkulaufzeit |
| **Quiet** | powersave | power | 10 % | 40 W³ | Auto | Leises Arbeiten |
| **Balanced** | powersave | balance_performance | 17 % | 80 W³ | Auto | Allgemeine Nutzung |
| **Performance** | powersave¹ | performance | 50 % | 100 W³ | Max | Gaming |
| **Turbo** | performance² | 0 (vom Kernel erzwungen) | 100 % | 110 W³ | Max | Maximale Leistung |

Die Auswahl eines beliebigen Profils wendet auch dessen Lüftermodus an, ein separater Schritt ist nicht nötig.
Die Wahl von Performance oder Turbo stellt den Lüfter auf Max (wie die physische
Turbo-Taste); Quiet, Balanced und Eco belassen ihn auf Auto.

⁴ Nur im Akkubetrieb, passend zur offiziellen Windows-App: Eco wird im Netzbetrieb
niemals als Option angeboten, weshalb die Karte nur auf der Mode-Seite erscheint,
wenn das Netzteil nicht angeschlossen ist. Für diese Stufe existieren keine
bestätigten Acer-Watt-/EPP-Werte, daher sind ihre Einstellungen eine
konservative Extrapolation unterhalb der eigenen Werte von Quiet, kein
gemessener Wert wie bei den anderen vier Profilen.

¹ Die HWP-Richtlinie `powersave` von Intel P-State ist ein dynamischer
Skalierungsalgorithmus, nicht der generische Governor mit Mindestfrequenz. Sie
hält das modellspezifisch benannte EPP beschreibbar, wodurch Performance zu
einer dynamischen Stufe von 50 % bis zum Maximum wird.

² Die HWP-Richtlinie `performance` selbst erzwingt EPP 0 und beschränkt den
verfügbaren P-State-Bereich auf dessen obere Grenze. Predator Sense verlässt
sich auf dieses Kernelverhalten, statt numerische EPP-Schreibvorgänge zu
verlangen. Das Backend wird anhand jeder cpufreq-Richtlinie erkannt, ohne eine
Zulassungsliste für CPU-Modelle. Andere Treiber behalten die bestehende
Zuordnung von `performance` zum benannten `performance` bei, und Systeme ohne
EPP überspringen nur diese optionale Steuerung.

³ Nach bestem Bemühen über `nvidia-smi -pl`, ebenso wie der Schieberegler für
das Leistungslimit im GPU-Dashboard weiter unten: wird stillschweigend
übersprungen, wenn `nvidia-smi` nicht vorhanden ist, und bei manchen Notebooks
gibt das vBIOS die NVML-Leistungslimit-Steuerung überhaupt nicht frei
(`nvidia-smi -q` meldet `Power Management Object: N/A`, jeder `-pl`-Wert wird
unabhängig von der Anfrage abgelehnt). Das ist eine Einschränkung auf
Firmware-Ebene, die weder diese App noch irgendeine Linux-Software ändern
kann; sie anzuheben würde bedeuten, mit einem reinen Windows-Tool wie
`nvflash` ein anderes vBIOS zu flashen, mit einem realen Risiko, die GPU zu
bricken, und liegt ganz in der eigenen Verantwortung des Besitzers.

**Bekannter Unterschied zur offiziellen Windows-App:** Bei Quiet schaltet das
offizielle PredatorSense zusätzlich den NVIDIA-Whisper-Modus ein
(`NvAPI_NvToppsJpacSetControl`), der die Bildrate auf 60 FPS begrenzt, damit
die Lüfterkurve leiser laufen kann. Diese Funktion ist Teil der reinen
Windows-Treiber-API von NVIDIA und hat unter Linux keine Entsprechung, daher
ist Quiet hier unter Last nicht so leise wie Quiet unter Windows auf
derselben Hardware; dies ist eine Plattformeinschränkung, kein Fehler dieser
App.

### Firmware-Leistungsprofile (gemessen, nicht geraten)

Alles in der obigen Tabelle verteilt lediglich ein bestehendes Leistungsbudget
zwischen CPU und GPU neu. **Das Paket-Leistungslimit selbst** wird vom
eigenen Thermalprofil der Firmware festgelegt, und bei manchen Modellen
startet die Firmware in ihrem niedrigsten Profil: Dann hebt keine Änderung
an Governor, EPP oder `min_perf` die Obergrenze auch nur um ein Watt an.

`platform_profile` kann diese Modi nicht immer erreichen. Der Kernel-Treiber
benennt sie anhand einer festen Tabelle (`BALANCED=0, QUIET=1, PERFORMANCE=2,
TURBO=3, ECO=4`), die nicht für jede Firmware gilt. Gemessen an einem
Predator PHN16-73 (Arrow Lake, BIOS V1.26), durch Schreiben jedes rohen
Index und Rücklesen des Paketlimits:

| Firmware-Index | Dauerhaft (PL1) | Spitze (PL2) | Name über `platform_profile` |
|---:|---:|---:|---|
| 6 | 45 W | 50 W | *(keiner, nicht erreichbar)* |
| 0 | 55 W | 160 W | `balanced` |
| 1 | 70 W | 160 W | `quiet` |
| 4 | 95 W | 160 W | `low-power` |
| 5 | **115 W** | 160 W | *(keiner, nicht erreichbar)* |

Der stärkste und der schwächste Modus haben überhaupt keinen Namen, und die
drei benannten sind in der falschen Reihenfolge beschriftet. Eine korrigierte
Tabelle fest zu codieren würde das Problem nur auf die nächste Firmware
verschieben, daher **misst** Predator Sense stattdessen:

1. Das Kernelmodul stellt den rohen Index und die eigene Bitmaske der
   unterstützten Indizes der Firmware als
   `/sys/devices/platform/acer-wmi/thermal_profile` und
   `thermal_profile_supported` bereit.
2. **Mode → Profile kalibrieren** schreibt jeden unterstützten Index und
   liest das resultierende Paketlimit aus `intel-rapl-mmio`, dann werden sie
   nach dauerhafter Leistung sortiert. Dauert einige Sekunden und bewegt
   während der Ausführung hörbar die Lüfter.
3. Ab diesem Zeitpunkt steuern die vier obigen Stufen auch das
   Firmware-Profil, verankert so, dass Quiet auf dem tatsächlich schwächsten
   und Turbo auf dem tatsächlich stärksten landet.

Hinweise:

- **Geräte ohne lesbares RAPL** (AMD-Modelle, ältere Intel-Systeme) können
  nicht eingestuft werden. Die Profile werden weiterhin aufgelistet und
  können manuell umgeschaltet werden, aber die vier Stufen lassen die
  Firmware bewusst in Ruhe, statt eine Reihenfolge zu erraten: Bei der
  Firmware oben würde ein Raten anhand des Index Turbo auf das 45-W-Profil
  legen.
- Die Firmware **vergisst** das Profil bei jedem Netzzyklus, daher wendet der
  Boot-Dienst das zuletzt gewählte erneut an.
- Bei Modellen, bei denen die Firmware die Tastaturbeleuchtung an den
  Energiemodus koppelt, malt jeder Wechsel, einschließlich jedes Schritts
  einer Kalibrierung, die Tastatur neu. Das erledigt die Firmware, nicht
  diese App; wenn es Sie stört, wenden Sie Ihre Farben anschließend erneut
  über die Seite Beleuchtung an.
- Die physische **Modus-Umschalttaste** durchläuft dieselbe gemessene
  Reihenfolge, siehe unten.

### Physische Modus-Umschalttaste

Manche Modelle besitzen eine eigene Taste, die durch die Energiemodi
schaltet. Sie meldet sich **ausschließlich** als roher HID-Eingabebericht des
Embedded Controllers und erzeugt überhaupt kein Ereignis im
Input-Subsystem, weshalb sie unter Linux tot erscheint, während die
PredatorSense-Taste (ein WMI-Hotkey) funktioniert.

Der Daemon überwacht dafür das Acer-EC-HID-Gerät. Die Standardwerte wurden
an einem PHN16-73 erfasst (`1025:174B`, Report `04 85 ff`); andere Modelle
weichen voraussichtlich ab, daher lassen sich beide ohne Neukompilierung
überschreiben:

`~/.config/predator-sense/mode_key.json`:

```json
{ "product": "0000ABCD", "report": [4, 133, 255] }
```

(striktes JSON: Ein `//`-Kommentar in dieser Datei macht sie unparsbar, und
der Daemon fällt mit einem Vermerk in seinem Log auf die Standardwerte
zurück.)

Falls Ihre Taste nichts bewirkt, protokolliert der Daemon beim Start jedes
gefundene Acer-HID-Gerät (aktivieren Sie `debug_logging` in den
Einstellungen). Finden Sie das richtige Gerät mit
`sudo hexdump -C /dev/hidrawN`, während Sie die Taste drücken, und tragen Sie
es dann in die Datei ein; und eröffnen Sie bitte ein Issue mit den Werten,
damit diese als Standard für Ihr Modell ausgeliefert werden können.

Die Firmware verweigert außerdem den Moduswechsel unterhalb von 40 % Akku;
der Daemon meldet dies, statt die Taste defekt wirken zu lassen.

### Automatisches Profil je nach Energiequelle

Wenn in den Einstellungen aktiviert (bei Neuinstallationen standardmäßig
aktiv), ist dies nicht nur eine Reaktion auf das An- und Abstecken des
Netzteils, sondern wird fortlaufend durchgesetzt:
- **Im Netzbetrieb:** immer Performance oder Turbo. Ist eines dieser beiden
  bereits aktiv, wird es belassen; die automatische Umschaltung widerspricht
  nie einer manuellen Wahl zwischen beiden.
- **Im Akkubetrieb:** immer Balanced oder Quiet, niemals Performance/Turbo.
  Unter 15 % Akkuladung wird Quiet erzwungen, unabhängig vom konfigurierten
  Ziel.

### GPU-Dashboard

Echtzeit-Überwachung der NVIDIA-GPU:
- Temperatur, Auslastung, VRAM-Nutzung, Leistungsaufnahme (runde Anzeigen)
- Live-Verlaufsdiagramme für Temperatur und Auslastung (2-Minuten-Fenster)
- Kerntakt, Speichertakt, P-State, PCIe-Verbindungsinformationen, VBIOS-Version

### KI-Assistent (beta)

Ein optionaler lokaler KI-Assistent auf Basis von [Ollama](https://ollama.com), der vollständig auf Ihrem eigenen Rechner läuft: Es wird nichts irgendwohin gesendet.

1. Installieren Sie Ollama separat gemäß den [offiziellen Linux-Anweisungen](https://ollama.com/download/linux)
2. Gehen Sie in der Seitenleiste zu **AI** und laden Sie über die integrierte Modellverwaltung ein Modell herunter (`smollm2:1.7b` oder größer; kleinere Modelle unterstützen Tool-Calling nicht zuverlässig)
3. Aktivieren Sie den Assistenten in den **Einstellungen** und wählen Sie **Auto-apply** (wendet Vorschläge sofort an) oder **Always confirm** (Standard: jede vorgeschlagene Änderung wartet auf Ihre Zustimmung)

Der Assistent liest den aktuellen Hardwarezustand (Temperatur, Lüfter, Thermalprofil, Akku) und kann Änderungen über eine feste, bereits validierte Menge an Aktionen vorschlagen oder anwenden; er greift nie direkt auf rohe Hardware-/EC-Zugriffe zu, und jede Aktion entspricht 1:1 einer Funktion, die diese App bereits verwendete, bevor die KI-Funktion existierte. Das Modell wird nur zur Durchführung einer Analyse geladen und anschließend wieder entladen; es liegt nicht untätig im Speicher. Alle KI-Aktivitäten werden auf derselben Seite in einem dauerhaften, überprüfbaren Aktionsprotokoll festgehalten.

---

## Installer-Optionen

Der Rust-Installer bietet eine interaktive TUI:

```console
sudo ./predator-sense-installer              # Interaktives Menü
sudo ./predator-sense-installer --install    # Direkte vollständige Installation
sudo ./predator-sense-installer --uninstall  # Alles entfernen
sudo ./predator-sense-installer --reload-module # Kernelmodul neu erstellen/neu laden
sudo ./predator-sense-installer --status     # Komponentenstatus anzeigen
```

---

## Deinstallation

```console
sudo ./predator-sense-installer  # Option 2 wählen
```

Oder manuell:
```console
pkill -f "/opt/predator-sense/predator-sense"
sudo rm -rf /opt/predator-sense
sudo rm -f /usr/share/applications/predator-sense.desktop
sudo rm -f /usr/share/icons/hicolor/128x128/apps/predator-sense.png
rm -f ~/.config/systemd/user/predator-sense-hotkey.service
rm -f ~/.config/autostart/predator-sense-hotkey.desktop
sudo rmmod facer  # Optional: Kernelmodul entladen
```

---

## Fehlerbehebung

<details>
<summary><b>Tastatur-RGB ändert sich nicht / bleibt bei einem Effekt hängen</b></summary>

Der Zustand des Kernelmoduls könnte feststecken. Laden Sie es neu:
```console
sudo rmmod facer
sudo insmod /path/to/kernel/facer.ko
# Oder nutzen Sie den Installer: sudo ./predator-sense-installer → Option 4
```
</details>

<details>
<summary><b>Modul lädt nicht</b></summary>

```console
# Prüfen, ob das WMI-Gerät existiert
ls /sys/bus/wmi/devices/7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56/

# Kernel-Logs prüfen
sudo dmesg | grep -i facer

# Sicherstellen, dass die Header zu Ihrem Kernel passen
sudo apt install linux-headers-$(uname -r)
```
</details>

<details>
<summary><b>PredatorSense-Taste funktioniert nicht</b></summary>

```console
# Den Rust-Hotkey-Dienst prüfen
systemctl --user status predator-sense-hotkey.service
pgrep -af predator-sense-hotkey

# Sicherstellen, dass der Benutzer in der Gruppe 'input' ist (vollständiges Ab-/Anmelden oder Neustart nach dem Hinzufügen erforderlich)
groups | grep input
sudo usermod -aG input $USER
```
</details>

<details>
<summary><b>NVIDIA-GPU-Seite zeigt keine Daten</b></summary>

```console
# Prüfen, ob nvidia-smi funktioniert
nvidia-smi
# Falls nicht, die proprietären NVIDIA-Treiber installieren
```
</details>

<details>
<summary><b>Mein Modell hat keinen passenden Quirk-Eintrag (fehlende Profile/Lüfterauslesung/PWM)</b></summary>

Falls Ihr genaues Modell noch nicht in der Kompatibilitätsliste steht, versuchen Sie, alle optionalen Funktionen der `predator_v4`-Familie zu erzwingen, und sehen Sie, was auf Ihrer Hardware tatsächlich funktioniert:

```console
sudo modprobe facer enable_all=1
# dauerhaft über Neustarts hinweg:
echo "options facer enable_all=1" | sudo tee /etc/modprobe.d/facer-options.conf
```

Dies ist rein WMI-basiert (keine rohen EC-Schreibzugriffe), daher ist es auf Hardware, die eine bestimmte Funktion nicht implementiert, ein sicherer No-Op statt eines schädlichen Schreibvorgangs. Bitte [eröffnen Sie ein Issue](https://github.com/cleyton1986/predator-sense/issues) mit Ihrem Modell und dem, was funktioniert hat bzw. nicht funktioniert hat; so werden neue Quirk-Einträge ergänzt.
</details>

---

## Projektstruktur

```
predator-sense-gui/
├── kernel/                      # Linux-Kernelmodule (verwaltet über DKMS)
│   ├── facer.c                  # ACPI/WMI-Schnittstelle zur Acer-Hardware
│   ├── acer-wmi-battery.c       # Unterstützung für das Akku-Ladelimit
│   ├── acpi_ec.c                # Roher EC-Zugriff über /dev/ec (von MusiKid/acpi_ec)
│   ├── Makefile
│   └── dkms.conf                # DKMS-Konfiguration für automatisches Neuerstellen
├── installer/                   # Rust-Multicall-Installer und Dienste
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # Typisiertes Dispatch nach installiertem Ausführbarkeitsnamen
│       ├── constants.rs         # Zentrale Pfade, Protokollwerte und Hardware-Konstanten
│       ├── install.rs           # Installer + DKMS-Registrierung
│       ├── helper.rs            # Validierte, privilegierte Hardware-Operationen
│       ├── hotkey.rs            # Linux-Input-Event-Listener
│       ├── tray.rs              # StatusNotifierItem-Dienst
│       └── i18n.rs              # Typisierte EN/PT-Meldungen
├── protocol/                    # Gemeinsamer, typisierter GUI-/Helper-Vertrag
│   ├── Cargo.toml
│   └── src/lib.rs               # Aktionen, Pfade, Limits und Binärnamen
├── src/                         # Rust-GTK4-Anwendung
│   ├── main.rs
│   ├── app_state.rs             # Globales Flag für die Fenstersichtbarkeit (steuert Timer)
│   ├── i18n.rs                  # EN/PT-Internationalisierung
│   ├── config.rs                # Benutzereinstellungen (JSON)
│   ├── tray.rs                  # Lebenszyklus des Rust-Tray-Dienstes
│   ├── hardware/
│   │   ├── helper.rs            # Typisierter Client für den privilegierten Helper
│   │   ├── rgb.rs               # RGB über /dev/acer-gkbbl-*
│   │   ├── hwmon.rs             # Index von /sys/class/hwmon (im OnceLock zwischengespeichert)
│   │   ├── sensors.rs           # Temperaturen, Lüfter, RAM, Netzwerk
│   │   ├── gpu.rs               # nvidia-smi-Parser mit TTL-Cache
│   │   ├── procs.rs             # /proc-Sampler (CPU pro Kern, Speicher, Prozessliste)
│   │   ├── storage.rs           # Datenträgernutzung über df
│   │   ├── sysinfo.rs           # DMI- + CPU- + GPU- + Betriebssystemdaten
│   │   ├── fan.rs               # Lüftermodus + CoolBoost
│   │   ├── extras.rs            # Akkulimit, LCD-Overdrive, USB-Laden, Boot-Animation
│   │   ├── profile.rs           # CPU-Governor + EPP + GPU-Leistung
│   │   ├── ai_assistant.rs      # Ollama-Tool-Calling: feste Zulassungsliste, abgebildet auf bestehende hardware::-Setter
│   │   ├── ai_snapshot.rs       # Flüchtiger Hardwarezustands-Schnappschuss für die KI, nach jedem Lesen gelöscht
│   │   ├── ai_actionlog.rs      # Dauerhaftes, überprüfbares Protokoll allem, was die KI vorgeschlagen/angewendet hat
│   │   └── setup.rs             # Verwaltung des Kernelmoduls
│   └── ui/                      # GTK4-Seiten (benutzerdefinierte Cairo-Widgets)
│       ├── window.rs            # Hauptfenster, Seitenleiste, Neon-Balken, Ausblenden in den Tray
│       ├── dashboard_page.rs    # Hero-Bereich + Systemspezifikationen
│       ├── temperatures_page.rs # Alle Temperaturanzeigen
│       ├── usage_page.rs        # CPU/GPU/Speicher/Datenträger mit ressourcenintensivsten Prozessen
│       ├── network_page.rs      # Download/Upload mit Spitzenwertverfolgung
│       ├── rgb_page.rs          # Tastatur-RGB mit visuellen Zonen
│       ├── fan_control_page.rs  # Animierte Lüfter + CoolBoost
│       ├── fan_page.rs          # Leistungsprofile
│       ├── battery_page.rs      # Akkustatistiken + Ladelimit
│       ├── gpu_page.rs          # NVIDIA-GPU-Dashboard
│       ├── monitor_page.rs      # Detaillierte CPU-/GPU-Verlaufsdiagramme
│       ├── ai_page.rs           # KI-Assistent: Chat, Modellverwaltung, Ressourcenmonitor, Aktionsprotokoll
│       ├── setup_page.rs        # Einrichtungsassistent für das Kernelmodul
│       └── gauge_widget.rs      # Widget für gestrichelte runde Anzeige
└── resources/
    ├── style.css                # Dunkles Gaming-Design
    └── predator-icon.svg        # System-Tray-Symbol
```

---

## Danksagungen

- **Kernelmodul `facer`** basierend auf dem Projekt [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) von [JafarAkhondali](https://github.com/JafarAkhondali) und [allen Mitwirkenden](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module/graphs/contributors)
- **Kernelmodul `acpi_ec`** von [Sayafdine Said (MusiKid)](https://github.com/MusiKid/acpi_ec): stellt `/dev/ec` für rohes EC-Lesen/-Schreiben bereit. Wird vom Helper verwendet, um Lüftermodi, CoolBoost, LCD-Overdrive, USB-Laden und die Boot-Animation zu steuern.
- **GUI-Anwendung** entwickelt mit [Rust](https://www.rust-lang.org/) + [GTK4](https://gtk.org/) + [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/)
- **Installer und Hintergrunddienste** entwickelt mit [Rust](https://www.rust-lang.org/); die Tray-Integration nutzt [ksni](https://crates.io/crates/ksni)
- **Symbole für Dashboard und Temperaturen** (`predator-sense-gui/resources/icons/`) von [Flaticon](https://www.flaticon.com), erstellt von Hilmy Abiyyu A., magnific und mehwish

### Dieses Projekt forken oder weiterverwenden

Dieses Projekt steht unter der GPL-3.0-Lizenz, Sie können es also frei forken, verändern und unter derselben Lizenz weiterverbreiten. Wenn Sie das tun, insbesondere wenn Sie eine abgeleitete App entwickeln oder wesentliche Teile der GUI/des Kernelmoduls weiterverwenden, **belassen Sie bitte einen sichtbaren Hinweis auf den ursprünglichen Autor** (eine Erwähnung von [Cleyton Alves](https://github.com/cleyton1986) bzw. dieses Repositorys in Ihrer README, im Über-Bildschirm oder im Danksagungsbereich reicht völlig aus). Es ist eine kleine Bitte, die für ein unabhängiges, unbezahltes Nebenprojekt viel bewirkt.

## Das Projekt unterstützen

Wenn Ihnen dieses Projekt nützlich war und Sie die Weiterentwicklung unterstützen möchten, können Sie mir gerne einen Kaffee spendieren:

<p align="center">
  <a href="https://www.paypal.com/cgi-bin/webscr?cmd=_donations&business=cleyton1986%40gmail.com&currency_code=BRL&item_name=Predator+Sense+for+Linux">
    <img src="https://img.shields.io/badge/PayPal-Donate-00457C?logo=paypal&logoColor=white&style=for-the-badge" alt="Donate via PayPal">
  </a>
</p>

<p align="center">
  <b>PIX (Brasilien):</b> <code>cleyton1986@gmail.com</code>
</p>

Jeder Beitrag ist freiwillig und wird sehr geschätzt! Er hilft dabei, das Projekt am Leben zu erhalten, und motiviert zu neuen Funktionen.

---

## Lizenz

Dieses Projekt steht unter der **GNU General Public License v3.0**, weitere Details finden Sie in der Datei [LICENSE](LICENSE).

Dies ist freie Software: Sie können sie unter den Bedingungen der von der Free Software Foundation veröffentlichten GNU GPL weiterverbreiten und/oder verändern.

**Ausnahme, Produktbilder:** Die obige GPLv3-Lizenz deckt nur den Quellcode dieses Projekts ab. Die Acer-Predator-/Nitro-Notebookfotos unter `predator-sense-gui/resources/models/` sind Produktbilder Dritter (siehe [Haftungsausschluss](#haftungsausschluss) oben) und **nicht** von der GPLv3-Gewährung abgedeckt; alle Rechte an diesen Bildern verbleiben bei Acer Inc. und/oder den ursprünglichen Fotografen.

**Diese Software wird „wie besehen" bereitgestellt, ohne jegliche Gewährleistung.** Die Autoren sind nicht verantwortlich für etwaige Schäden, die durch die Nutzung dieser Software entstehen können. Durch die Installation und Nutzung dieser Software erkennen Sie an, dies auf eigenes Risiko zu tun.
