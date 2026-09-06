# Predator Sense per Linux

<p align="center">
  <a href="README.md">🇺🇸 Read in English</a> · <a href="README-ptbr.md">🇧🇷 Leia em Português</a> · <a href="README-es.md">🇪🇸 Leer en Español</a> · <a href="README-zh.md">🇨🇳 阅读中文版</a> · <a href="README-ja.md">🇯🇵 日本語で読む</a> · <a href="README-ru.md">🇷🇺 Читать на русском</a> · <a href="README-de.md">🇩🇪 Auf Deutsch lesen</a> · <a href="README-tr.md">🇹🇷 Türkçe Oku</a>
</p>

<p align="center">
  <img src="predator-sense-gui/resources/logo.jpeg" width="120" alt="Predator Sense Logo">
</p>

<p align="center">
  <b>Modulo kernel Linux non ufficiale e GUI per il controllo hardware dei notebook gaming Acer</b><br>
  <i>Retroilluminazione RGB della Tastiera &bull; Modalità Turbo &bull; Monitoraggio della Temperatura &bull; Profili di Prestazioni</i>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Lingua-Rust-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/GTK-4-blue?logo=gtk" alt="GTK4">
  <img src="https://img.shields.io/badge/Userspace-100%25_Rust-orange?logo=rust" alt="Userspace 100% Rust">
  <img src="https://img.shields.io/badge/Licenza-GPL--3.0-green" alt="License">
  <img src="https://img.shields.io/badge/Piattaforma-Linux-yellow?logo=linux" alt="Linux">
</p>

<p align="center">
  Creato e mantenuto da <a href="https://github.com/cleyton1986">Cleyton Alves</a>
</p>

---

## Avviso legale

> **Attenzione**
> **Usalo a tuo rischio e pericolo!** Questo è un progetto **non ufficiale**. Acer non è stata coinvolta nel suo sviluppo. Il modulo del kernel è stato sviluppato tramite reverse engineering dell'applicazione ufficiale PredatorSense per Windows. Questo driver interagisce con metodi WMI/ACPI di basso livello che non sono stati testati su tutte le serie di notebook. Gli autori non sono responsabili per eventuali danni al tuo hardware.

> **Nota**
> Tutti i marchi, i nomi di prodotto e i loghi menzionati (Acer, Predator, PredatorSense, Helios, Nitro, AeroBlade, CoolBoost) sono di proprietà dei rispettivi titolari (Acer Inc.). Questo progetto non è affiliato, approvato o sponsorizzato da Acer Inc. in alcun modo.

> **Immagini dei prodotti**
> Le foto dei notebook in `predator-sense-gui/resources/models/` ritraggono prodotti ufficiali Acer Predator/Nitro e vengono usate esclusivamente per permettere all'app di identificare visivamente il modello rilevato sulla macchina dell'utente stesso (confrontandolo con il `product_name` riportato dalla DMI/BIOS del sistema). Queste immagini **non sono coperte dalla licenza GPLv3 di questo progetto** — il copyright delle fotografie dei prodotti appartiene ad Acer Inc. e/o ai loro creatori originali. Sono incluse qui in buona fede, su base non commerciale e puramente informativa (uso nominativo/di identificazione del prodotto), senza alcuna rivendicazione di proprietà da parte di questo progetto. Se sei il titolare dei diritti e desideri che un'immagine venga rimossa, apri una issue e sarà rimossa prontamente.

Questa applicazione è stata creata per **uso personale**, per ottenere il massimo da un notebook gaming Acer su Linux — dato che Acer non offre supporto ufficiale per PredatorSense su Linux. Viene condivisa liberamente per chiunque voglia lo stesso.

Se questa app/progetto ti è stata utile e/o ti è piaciuta in qualche modo, considera di lasciare una stella, aiuta molto ⭐

---

## Screenshot

<p align="center"><b>Dashboard</b> — Foto del notebook e specifiche di sistema complete a colpo d'occhio: CPU, GPU, RAM, storage, rete e sistema operativo.</p>
<p align="center"><img src="assets/psense-1.png" width="800" alt="Dashboard"></p>

<p align="center"><b>Temperature</b> — Gauge in tempo reale per CPU, GPU, sistema, dischi NVMe, WiFi e RAM, tutto in un'unica schermata.</p>
<p align="center"><img src="assets/psense-2.png" width="800" alt="Temperature"></p>

<p align="center"><b>Utilizzo</b> — CPU, GPU, memoria e storage con i processi principali, barre animate e dettagli espandibili al click (con un'animazione di fuoco in stile CSS sul gauge della temperatura).</p>
<p align="center"><img src="assets/psense-3.png" width="800" alt="Utilizzo"></p>

<p align="center"><b>Rete</b> — Grafici di download/upload in tempo reale con tracciamento dei picchi e rilevamento automatico dell'interfaccia (Wi-Fi o Ethernet).</p>
<p align="center"><img src="assets/psense-4.png" width="800" alt="Rete"></p>

<p align="center"><b>Illuminazione</b> — Colori statici per zona (4 sezioni) ed effetti RGB dinamici della tastiera (Breathing, Neon, Wave, Shifting, Zoom).</p>
<p align="center"><img src="assets/psense-5.png" width="800" alt="Illuminazione"></p>

<p align="center"><b>Modalità</b> — Profili di prestazioni: Silenzioso, Bilanciato, Prestazioni e Turbo, più un livello Eco esclusivo per la batteria (CPU governor + Intel EPP + limite di potenza della GPU).</p>
<p align="center"><img src="assets/psense-6.png" width="800" alt="Modalità"></p>

<p align="center"><b>GameSync</b> — Registra un gioco e il suo profilo; l'app passa automaticamente a quel profilo mentre il gioco è in esecuzione e ripristina quello attivo in precedenza non appena si chiude.</p>
<p align="center"><img src="assets/psense-15.png" width="800" alt="GameSync"></p>

<p align="center"><b>Controllo Ventole</b> — RPM in tempo reale con ventole animate, interruttore CoolBoost e modalità Auto/Max.</p>
<p align="center"><img src="assets/psense-7.png" width="800" alt="Controllo Ventole"></p>

<p align="center"><b>Batteria</b> — Percentuale di carica, voltaggio, corrente, potenza, cicli, salute, produttore e limite di carica all'80% per la longevità.</p>
<p align="center"><img src="assets/psense-8.png" width="800" alt="Batteria"></p>

<p align="center"><b>GPU</b> — Dashboard NVIDIA con grafici in tempo reale, clock, utilizzo, VRAM, consumo energetico e info PCIe.</p>
<p align="center"><img src="assets/psense-9.png" width="800" alt="GPU"></p>

<p align="center"><b>Grafici</b> — Grafici storici dettagliati di CPU e GPU con tracciamento di minimi/massimi.</p>
<p align="center"><img src="assets/psense-10.png" width="800" alt="Grafici"></p>

<p align="center"><b>Assistente IA (beta)</b> — Assistente IA locale basato su Ollama: chat, gestione modelli (elenca i modelli installati, scaricane di nuovi, scegli quale eseguire), utilizzo di risorse VRAM/GPU in tempo reale mentre elabora e un log delle azioni persistente.</p>
<p align="center"><img src="assets/psense-11.png" width="800" alt="Assistente IA"></p>

<p align="center"><b>Driver e manuali</b> — Mostra il numero di serie (con un pulsante per copiarlo) e un link diretto alla pagina ufficiale driver e manuali di Acer, oltre a un'illustrazione di dove trovare l'etichetta del numero di serie sul notebook.</p>
<p align="center"><img src="assets/psense-16.png" width="800" alt="Driver e manuali"></p>

<p align="center"><b>Impostazioni</b> — Riduci a icona nel tray, avvio all'accensione, applicazione automatica del profilo all'avvio, preferenze di lingua ed elenco delle funzionalità supportate per modello.</p>
<p align="center"><img src="assets/psense-12.png" width="800" alt="Impostazioni"></p>

<p align="center"><b>Illuminazione del logo della cover</b> — Controllo RGB indipendente per il logo sul retro del display, sui modelli con logo della cover a colori (Static/Breathing/Neon). Rilevato a runtime: il controllo appare solo se l'hardware risponde a una verifica delle capacità, quindi resta nascosto in sicurezza sui modelli che non lo supportano.</p>
<p align="center"><img src="assets/psense-13.png" width="800" alt="Illuminazione del logo della cover"></p>
<p align="center"><img src="assets/psense-14.jpg" width="800" alt="Logo della cover acceso in verde su un Predator PHN16-73"></p>
<p align="center"><sub>Funzionalità contribuita da <a href="https://github.com/jlucaso1">@jlucaso1</a>, testata sul suo Predator PHN16-73. Il logo della cover di questo notebook non supporta i colori, quindi la funzionalità è stata verificata usando il suo hardware.</sub></p>

---

## Informazioni

Modulo kernel Linux non ufficiale per la retroilluminazione RGB della tastiera e la modalità Turbo dei notebook gaming Acer (Acer Predator, Acer Helios, Acer Nitro).

Ispirato e basato sul progetto [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) di [JafarAkhondali](https://github.com/JafarAkhondali) e collaboratori. Questo progetto estende l'attuale modulo kernel Linux Acer-WMI per supportare le funzioni gaming Acer, e aggiunge un'**applicazione desktop GUI completa** sviluppata con Rust e GTK4.

---

## Funzionalità

| Funzionalità | Descrizione |
|---------|-------------|
| **Dashboard** | Foto del notebook + specifiche di sistema complete (CPU, GPU, RAM, storage, rete, SO) |
| **Temperature** | Gauge in tempo reale per CPU, GPU, sistema, NVMe, WiFi e RAM |
| **Utilizzo** | Vista a 4 schede: CPU / GPU / Memoria / Storage con i processi principali, dettagli espandibili al click e animazione di fuoco in stile CSS sui gauge di temperatura |
| **Rete** | Grafici di download/upload in tempo reale con tracciamento dei picchi e rilevamento automatico dell'interfaccia |
| **Controllo RGB Tastiera** | Colori statici per zona (4 zone) ed effetti dinamici (Breathing, Neon, Wave, Shifting, Zoom) via WMI. Su hardware senza il modulo kernel, l'RGB funziona invece nativamente via USB/I2C-HID — chip ENEK5130 (statico a 4 zone, Breathing/Neon), chip Sunrex 2024+ (zona singola, elenco completo di effetti) o chip Chicony (palette a 7 colori, Helios 300) — auto-rilevato, vedi [Compatibilità](#compatibilità) |
| **Logo RGB della Cover** | Controlli indipendenti di accensione, colore fisso, luminosità, Breathing e Neon per l'emblema sul retro del display, con anteprima vettoriale in tempo reale. Mostrato solo dopo il rilevamento a runtime delle capacità HID |
| **Profili di Prestazioni** | Modalità Silenzioso / Bilanciato / Prestazioni / Turbo, più un livello Eco esclusivo per la batteria (CPU governor + Intel EPP + limite di potenza della GPU) |
| **Controllo Ventole** | RPM in tempo reale con ventole animate, interruttore CoolBoost, modalità Auto/Max, più controllo PWM per singola ventola e curva di temperatura automatica sperimentali (dove supportato) |
| **Batteria** | Statistiche di carica, cicli, salute, info produttore e limite di carica all'80% per la longevità |
| **Dashboard GPU** | Metriche NVIDIA: temperatura, utilizzo, VRAM, clock, consumo, info PCIe con grafici in tempo reale, più uno **slider per il limite di potenza (TGP)** |
| **Grafici** | Grafici storici dettagliati di CPU e GPU con tracciamento di minimi/massimi |
| **Assistente IA** 🧪 | Assistente IA locale e opzionale basato su [Ollama](https://ollama.com) — legge lo stato dell'hardware in tempo reale e suggerisce o applica modifiche tramite un insieme fisso di azioni già validate (profilo termico, modalità ventole, CoolBoost, RGB, limite di potenza GPU, batteria). Chat, gestione modelli (scaricare/selezionare), monitor risorse/VRAM in tempo reale e un log delle azioni persistente. Applicazione automatica o conferma sempre richiesta, a tua scelta. Richiede Ollama installato separatamente — vedi [Assistente IA](#assistente-ia-beta) più in basso |
| **Rilevamento automatico delle capacità** | Rileva cosa supporta ogni modello e adatta l'interfaccia — le funzionalità non supportate vengono mostrate come "non disponibile su questo modello" invece di generare un errore. Le funzionalità supportate sono elencate in Impostazioni |
| **Avvisi di temperatura** | Notifica desktop quando CPU/GPU superano i 90°C (funziona anche dal tray) |
| **Profilo energetico automatico** | Cambia profilo automaticamente al passaggio tra alimentazione e batteria — il profilo di destinazione per ogni stato è configurabile in Impostazioni (predefinito: Prestazioni con alimentazione, Bilanciato a batteria) |
| **Log di debug** | Interruttore opzionale in Impostazioni — registra gli eventi del daemon e dell'app in `~/.local/share/predator-sense/` (a rotazione, 5MB×3) per la diagnosi da remoto. Disattivato di default |
| **Tray di Sistema** | Riduci a icona nel tray con l'icona Predator — l'app resta attiva in background |
| **Tasto PredatorSense** | Mappatura del tasto hardware — il tasto accanto al NumLock apre l'app |
| **DKMS** | I moduli del kernel si ricompilano automaticamente ad ogni aggiornamento del kernel |
| **Internazionalizzazione** | Inglese / Portoghese automatico in base alla lingua di sistema |
| **Interfaccia Gaming** | Tema scuro con barre neon pulsanti, gauge circolari tratteggiati, bordi dei pannelli poligonali. Il colore d'accento segue automaticamente il brand rilevato — ciano su Predator/Helios/Triton, arancione/rosso su Nitro (come NitroSense) — nessuna impostazione da attivare manualmente |

---

## Compatibilità

**Funzionerà sul mio notebook?**

Legenda: ✅ testato e funzionante · 🟡 implementato, non testato (serve un tester) · 🧪 sperimentale (serve un tester) · ❌ non funziona · `-` non applicabile

| Nome Prodotto | Turbo (Impl.) | Turbo (Testato) | RGB (Impl.) | RGB (Testato) | Lettura RPM ventole | Profili ventole | Fan PWM % |
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

> Se il tuo modello non è elencato, potrebbe comunque funzionare — il modulo kernel rileva automaticamente le interfacce WMI compatibili. Se ha funzionato (o non ha funzionato) per te, apri una issue menzionando il tuo modello così possiamo aggiornare questa tabella.

### Controllo ventole — tre livelli

| Livello | Cosa fa | Disponibilità |
|---|---|---|
| **Lettura RPM ventole** | Legge la velocità delle ventole CPU/GPU (`fan1_input`, `fan2_input`) | Maggior parte dei modelli gaming (auto-rilevato) |
| **Profili ventole** | Silenzioso / Bilanciato / Prestazioni / Turbo via `platform_profile` | Modelli `predator_v4` |
| **Fan PWM %** 🧪 | Controllo della velocità per singola ventola (`pwm1`/`pwm2` 0–100%) portato dal driver mainline `acer-wmi` via WMI — **solo kernel ≥ 6.14** | Sottoinsieme di modelli con `ACER_CAP_PWM` (AN515-58, PHN16-72/73, PH16-72, …) |

> **🧪 Il controllo PWM delle ventole è sperimentale.** È portato dal driver `acer-wmi` del kernel Linux upstream e usa metodi WMI sicuri (nessuna scrittura raw sull'EC), ma **non è stato verificato su hardware reale** dal maintainer (che possiede un PH315-54, che non ha il PWM). Se hai un modello supportato, i resoconti di test sono benvenuti. **Usalo a tuo rischio e pericolo** — vedi l'avviso legale in cima.

### Alternativa: linuwu_sense (hardware senza quirk, con Turbo non funzionante)

Il fallback `enable_all=1` di `facer` riconosce qualsiasi scheda con WMI Acer, ma l'insieme completo di profili `predator_v4` (5 profili tra cui `balanced-performance`/`performance`, `turbo_state` scrivibile) si applica solo alle schede presenti nella sua tabella di quirk DMI. Su una scheda senza quirk, `platform_profile_choices` è limitato a `low-power quiet balanced` e `turbo_state` resta di sola lettura anche se il firmware supporta di più — segnalato su un'unità PHN16-73 (Macan_ARX, BIOS V1.26) nella [#33](https://github.com/cleyton1986/predator-sense/issues/33).

Se è il tuo caso, il modulo della community [Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense) (caricato con `predator_v4=1`) espone l'insieme completo di profili attraverso le stesse interfacce generiche `platform_profile`/`intel_pstate`/`acer-wmi-battery` che questa app già legge direttamente — nessun percorso di codice specifico di `facer` coinvolto. Dalla `v0.2.71-preview` l'app rileva `linuwu_sense` e salta l'avviso "installa facer" quando è quel driver a fornire effettivamente queste interfacce. L'RGB e la calibrazione del profilo termico (entrambi esclusivi di `facer`, vedi sopra e sotto) richiedono comunque `facer` stesso e restano non disponibili con linuwu_sense.

### RGB senza il modulo kernel (solo hardware I2C-HID)

Alcuni modelli (confermati: PHN16S-71, PHN16-73, AN16S-61) instradano il controller RGB della tastiera attraverso un chip I2C-HID separato (ENEK5130) invece dell'interfaccia WMI di `facer.ko` — l'app comunica con esso direttamente via `/dev/hidrawN`, quindi questi funzionano anche se il modulo kernel non è affatto caricato:

| Funzionalità | Stato |
|---|---|
| Colore statico per zona, luminosità, spegnimento retroilluminazione | ✅ confermato funzionante (PHN16S-71, AN16S-61) |
| Effetti dinamici — Breathing, Neon | ✅ confermato funzionante (PHN16S-71, AN16S-61) — nativo, singola scrittura HID, l'hardware ripete il pattern in loop da solo. Sull'unità PHN16S-71, Breathing ignora il colore scelto e cicla l'arcobaleno; può variare su altro hardware |
| Effetti dinamici — Wave, Shifting, Zoom | Solo anteprima a schermo (nessuna scrittura sull'hardware) — i codici di questi effetti si sono rivelati avere significati diversi tra le generazioni di hardware, quindi non sono ancora collegati |
| Logo RGB della cover — spento, colore fisso, luminosità, Breathing, Neon | ✅ confermato funzionante (PHN16-73) |

Il supporto al logo della cover non viene abilitato tramite una allow-list di nomi di modello. Il controller deve dichiarare il target `0x83` nel suo report A1 dei target e restituire capacità A3 corrispondenti e non vuote prima che l'interfaccia venga mostrata; l'app ripete questo controllo immediatamente prima di ogni scrittura. Il daemon dell'hotkey ripristina solo un'impostazione che l'app ha già applicato con successo dopo il login e il resume, e salta completamente il logo quando non c'è un'impostazione salvata o il target è assente.

Un [resoconto indipendente sull'AN16S-61](https://github.com/cleyton1986/predator-sense/issues/31) (vedi anche lo [strumento di protocollo standalone](https://github.com/ArnarValur/Nitro16S-AI-RGB-Keyboard) dello stesso autore) ha mappato altre sei modalità native via cavo oltre a static/Breathing/Neon/Wave (una modalità di spegnimento hardware, una modalità di lampeggio al boot innescata dallo stesso EC, e altre quattro animazioni integrate), più un target LED per il tasto di modalità/turbo. Nulla di tutto ciò è ancora collegato nell'app — serve prima uno slot definito per i codici di effetto nativi dell'hardware, quindi resta registrato come miglioramento futuro.

Lo stesso resoconto includeva anche un report descriptor HID decodificato preso direttamente dal controller, che ha risolto un bug reale: l'app leggeva il conteggio delle zone del report di capacità A3 dal byte sbagliato (`byte[3]`, una costante fissa per classe di target) invece del byte che il descriptor del controller stesso dichiara per questo scopo (`byte[4]`). Corretto nella `v0.2.69-preview` sia nell'app che nel daemon dell'hotkey. È una correzione a livello di protocollo, non un cambiamento per modello - il layout dei campi del report descriptor proviene dal firmware stesso del chip (stesso chip `0CF2:5130` su tutti e tre i modelli confermati) - e non cambia alcun byte sul cavo su hardware già confermato funzionante, dato che il valore precedente era sempre un superinsieme sovra-inclusivo di quello corretto.

### RGB su hardware 2024+ (Sunrex/Darfon USB HID)

Una generazione più recente (PH16-72 e altri modelli 2024-2026 che condividono gli stessi chip USB HID, vedi issue #26) ha spostato l'RGB di tastiera e logo della cover sia dalla WMI *sia* dal chip ENEK5130 sopra descritto, su una coppia di controller completamente diversa — Sunrex `05af:*` per la tastiera, Darfon `0d62:*` per il logo. L'app rileva e pilota direttamente anche questi, selezionati automaticamente al posto dei percorsi ENEK5130/WMI quando presenti:

| Funzionalità | Stato |
|---|---|
| Tastiera: Spento, Statico, Breathing, Wave, Snake, Neon, Spot, Star, Rainbow, 5× Slash, Zoom, Row Wave, Swiping | 🟡 implementato, in attesa di conferma su hardware reale |
| Logo della cover: spento, colore fisso, luminosità, Breathing | 🟡 implementato, in attesa di conferma su hardware reale |

Questo chip non ha zone indipendenti — l'intera tastiera usa un solo colore/effetto alla volta, a differenza del controller ENEK5130 a 4 zone descritto sopra. Il protocollo via cavo è stato decodificato byte per byte da due release decompilate dell'app ufficiale Windows (ogni sequenza di byte fissa e formula di checksum corrispondevano esattamente tra le due), non è una supposizione — ma nessuno lo ha ancora confermato su hardware fisico, quindi trattalo come non testato finché non arriva un resoconto reale.

Un terzo chip (Chicony, Helios 300/PH317-56) usa un ulteriore protocollo USB HID, documentato tramite reverse engineering della community ([NT411/Acer-Predator-Fan-RGB-Controller-Linux](https://github.com/NT411/Acer-Predator-Fan-RGB-Controller-Linux)) e reimplementato qui a partire da quella specifica — palette fissa di 7 colori (un limite hardware/firmware, non RGB arbitrario) su 12 effetti. Anche questo 🟡, in attesa di conferma.

### Stai già usando Linuwu-Sense o DAMX?

[Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense) (e [DAMX](https://github.com/PXDiv/Div-Acer-Manager-Max), che è costruito sopra di esso) è un progetto separato e non correlato che pilota anch'esso l'hardware Acer Predator/Nitro su Linux. Non è una dipendenza di questo progetto e nessun suo codice viene usato qui — ma il suo modulo kernel si aggancia agli **stessi GUID WMI** di cui ha bisogno `facer`, e il kernel non permette a due driver di reclamare lo stesso dispositivo contemporaneamente.

Se l'installer rileva `linuwu_sense` già caricato o installato via DKMS, **lascia automaticamente intatta** la tua configurazione esistente — non mette `acer_wmi` in blacklist né forza il caricamento di `facer`, così non entra in conflitto (né rompe) un'installazione Linuwu-Sense/DAMX già funzionante. L'RGB della tastiera continua a funzionare tramite questa app via il percorso HID (vedi sopra) indipendentemente da quale driver di piattaforma sia attivo; in questo caso il controllo di ventole/termico resta affidato allo strumento che già usavi per gestirlo.

---

## Installazione

### Installer Precompilato (Più Veloce)

Scarica direttamente l'installer della release ed eseguilo:

```console
curl --fail --location https://github.com/cleyton1986/predator-sense/releases/latest/download/predator-sense-installer --output predator-sense-installer
chmod +x predator-sense-installer
sudo ./predator-sense-installer --install
```

L'installer, l'helper privilegiato, il listener dell'hotkey e il servizio del tray sono tutti forniti dallo stesso binario multicall Rust. L'installer scarica e configura tutto senza un bootstrap tramite shell script.

### Installer Interattivo (binario precompilato, non serve toolchain Rust)

Scarica il binario `predator-sense-installer` dalla pagina [Releases](../../releases). È un binario Rust autonomo, non un pacchetto — necessita comunque di accesso a internet per scaricare il codice sorgente dell'app (per il modulo kernel) e il binario precompilato della release corrispondente, ma evita completamente di installare Rust e compilare l'app GTK4 sulla tua macchina:

```console
chmod +x predator-sense-installer
sudo ./predator-sense-installer
```

Seleziona l'**opzione 1** (Installazione completa). L'installer eseguirà automaticamente:

1. Rileva la tua distribuzione (Debian/Ubuntu/Mint, Fedora, Arch)
2. Installa le dipendenze di sistema (GTK4, libadwaita, strumenti di build, header del kernel)
3. Scarica il codice sorgente + binario precompilato della release corrispondente
4. Compila e carica il modulo kernel `facer` (questa parte compila sempre localmente — i moduli kernel non possono essere distribuiti precompilati tra versioni diverse del kernel)
5. Crea una voce nel menu applicazioni con icona
6. Mappa il tasto hardware PredatorSense (avvio automatico al login)
7. Configura il supporto al tray di sistema

Il percorso precompilato non richiede Rust/cargo sulla macchina di destinazione. L'installer viene anche copiato in `/opt/predator-sense/` come strumento di gestione autonomo per controlli di stato, ricariche del modulo kernel, aggiornamenti e disinstallazione (vedi [Opzioni dell'Installer](#opzioni-dellinstaller)).

Dopo l'installazione, apri l'app in uno di questi modi:
- Premendo il **tasto PredatorSense** (accanto al NumLock)
- Cercando **"Predator Sense"** nel menu delle applicazioni
- Eseguendo `/opt/predator-sense/predator-sense` in un terminale

### Installazione Manuale (Compilazione dal codice sorgente)

#### Prerequisiti

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

**Rust** (se non installato):
```console
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### Compilazione e Installazione

```console
# Clona il repository
git clone https://github.com/cleyton1986/predator-sense.git
cd predator-sense/predator-sense-gui

# Compila la GUI e l'installer/servizi Rust
cargo build --release
cargo build --release --manifest-path installer/Cargo.toml

# Installa la build locale e registra i sorgenti C esistenti in DKMS
sudo installer/target/release/predator-sense-installer --install

# Esegui
/opt/predator-sense/predator-sense
```

---

## Utilizzo

### RGB della Tastiera

1. Vai su **Illuminazione** nella barra laterale
2. Scegli **Statico** (colori per zona) o **Dinamico** (effetti)
3. **Modalità Statica:** regola gli slider R/G/B per ciascuna delle 4 sezioni della tastiera
4. **Modalità Dinamica:** seleziona un effetto (Breathing, Neon, Wave, Shifting, Zoom) e regola la velocità
5. Clicca su **Applica**

> Su hardware solo I2C-HID senza il modulo kernel (vedi [Compatibilità](#compatibilità)), Breathing e Neon animano davvero; Wave/Shifting/Zoom mostrano solo un'anteprima a schermo, chiaramente etichettata come tale — la tastiera fisica non cambia ancora per questi effetti.

### Logo RGB della Cover

1. Vai su **Illuminazione** e seleziona **Logo della cover** (il selettore appare solo quando viene rilevato un target HID compatibile)
2. Usa **Illuminazione** per accendere o spegnere l'emblema
3. Scegli **Statico**, **Breathing** o **Neon**, poi regola i controlli disponibili di colore, luminosità e velocità osservando l'anteprima in tempo reale
4. Clicca su **Applica al logo**

L'ultimo stato applicato con successo viene ripristinato quando si avvia il servizio hotkey dell'utente e dopo la sospensione/ibernazione. I colori degli effetti animati sono controllati dal firmware, quindi l'anteprima rappresenta il loro comportamento invece di offrire un selettore di colore per queste modalità.

> Il firmware controlla l'animazione di illuminazione mostrata prima che Linux avvii il servizio utente. Uno stato "spento" salvato viene ripristinato dopo il login, ma questa app non può sopprimere la precedente animazione del BIOS/boot.

### Profili di Prestazioni

Sui sistemi con Intel P-State + HWP attivi, il lato CPU si risolve così:

| Profilo | Politica HWP | Intel EPP | Prestazioni min. | Potenza GPU | Ventole | Caso d'uso |
|---------|------------|-----------|------------------|-----------|-----|----------|
| **Eco**⁴ | powersave | power | 5% | 25W³ | Auto | Massima autonomia della batteria |
| **Silenzioso** | powersave | power | 10% | 40W³ | Auto | Lavoro silenzioso |
| **Bilanciato** | powersave | balance_performance | 17% | 80W³ | Auto | Uso generale |
| **Prestazioni** | powersave¹ | performance | 50% | 100W³ | Max | Gaming |
| **Turbo** | performance² | 0 (forzato dal kernel) | 100% | 110W³ | Max | Prestazioni massime |

Selezionare un profilo qualsiasi applica anche la sua modalità ventole - non serve un passaggio separato.
Scegliere Prestazioni o Turbo porta le ventole al Max (come il tasto fisico
Turbo); Silenzioso, Bilanciato ed Eco le lasciano su Auto.

⁴ Esclusivo per la batteria, come l'app ufficiale Windows: non offre mai Eco
come opzione con l'alimentazione collegata, quindi la scheda compare nella
pagina Modalità solo quando scollegato. Non esistono numeri ufficiali Acer di
wattaggio/EPP confermati per questo livello, quindi le sue impostazioni sono
un'estrapolazione conservativa al di sotto dei valori dello stesso Silenzioso,
non un valore misurato come per gli altri quattro.

¹ La politica HWP `powersave` di Intel P-State è un algoritmo di scaling
dinamico, non il governor generico a frequenza minima. Mantiene scrivibile
l'EPP nominale specifico del modello, rendendo Prestazioni un livello dinamico
dal 50% fino al massimo.

² La stessa politica HWP `performance` forza l'EPP a 0 e limita l'intervallo
di P-state disponibile al suo limite superiore. Predator Sense si affida a
questo comportamento del kernel invece di richiedere scritture numeriche
dell'EPP. Il backend viene rilevato da ogni politica cpufreq, senza una
allowlist di modelli di CPU. Gli altri driver mantengono la mappatura
esistente `performance` + EPP nominale `performance`, e i sistemi senza EPP
saltano solo quel controllo opzionale.

³ Best-effort tramite `nvidia-smi -pl`, come lo slider del limite di potenza
del Dashboard GPU più sotto - saltato silenziosamente se `nvidia-smi` non è
presente, e su alcuni notebook la vBIOS non espone affatto il controllo del
limite di potenza di NVML (`nvidia-smi -q` riporta `Power Management Object:
N/A`, ogni valore di `-pl` viene rifiutato indipendentemente da cosa viene
richiesto). È un limite a livello di firmware, non qualcosa che questa app -
o qualsiasi software Linux - può cambiare; alzarlo significa flashare una
vBIOS diversa con uno strumento Windows-only come `nvflash`, un rischio reale
di brickare la GPU ed è una scelta esclusivamente del proprietario.

**Differenza nota rispetto all'app ufficiale Windows:** in Silenzioso, il
PredatorSense ufficiale attiva anche la Whisper Mode di NVIDIA
(`NvAPI_NvToppsJpacSetControl`), che limita il frame rate a 60 FPS per far
girare la curva delle ventole in modo più silenzioso. Questo controllo fa
parte dell'API del driver NVIDIA esclusiva di Windows e non ha un equivalente
su Linux, quindi qui Silenzioso non è silenzioso sotto carico quanto il
Silenzioso di Windows sullo stesso hardware - è un limite della piattaforma,
non un bug di questa app.

### Profili di Potenza del Firmware (misurati, non indovinati)

Tutto ciò che è nella tabella sopra ridistribuisce solo un budget di potenza
già esistente tra CPU e GPU. **Il limite di potenza del pacchetto** viene
impostato dal profilo termico del firmware stesso, e su alcuni modelli il
firmware si avvia nel profilo più basso — quindi nessuna modifica a governor,
EPP o `min_perf` alza il tetto nemmeno di un watt.

`platform_profile` non riesce sempre a raggiungere queste modalità. Il driver
del kernel le nomina a partire da una tabella fissa (`BALANCED=0, QUIET=1,
PERFORMANCE=2, TURBO=3, ECO=4`) che non vale per ogni firmware. Misurato su un
Predator PHN16-73 (Arrow Lake, BIOS V1.26), scrivendo ogni indice grezzo e
rileggendo il limite del pacchetto:

| Indice firmware | Sostenuto (PL1) | Burst (PL2) | Nome via `platform_profile` |
|---:|---:|---:|---|
| 6 | 45 W | 50 W | *(nessuno — irraggiungibile)* |
| 0 | 55 W | 160 W | `balanced` |
| 1 | 70 W | 160 W | `quiet` |
| 4 | 95 W | 160 W | `low-power` |
| 5 | **115 W** | 160 W | *(nessuno — irraggiungibile)* |

Le modalità più forte e più debole non hanno alcun nome, e le tre che ne hanno
uno sono etichettate nell'ordine sbagliato. Codificare una tabella corretta
sposterebbe solo il problema al firmware successivo, quindi Predator Sense
**misura invece**:

1. Il modulo kernel espone l'indice grezzo e la bitmask degli indici
   supportati dal firmware stesso come `/sys/devices/platform/acer-wmi/thermal_profile`
   e `thermal_profile_supported`.
2. **Modalità → Calibra profili** scrive ogni indice supportato e legge il
   limite di pacchetto risultante da `intel-rapl-mmio`, poi li ordina in base
   alla potenza sostenuta. Richiede pochi secondi e muove udibilmente le
   ventole durante l'esecuzione.
3. Da quel momento i quattro livelli sopra guidano anche il profilo del
   firmware, ancorati in modo che Silenzioso corrisponda al reale più debole
   e Turbo al reale più forte.

Note:

- **Le macchine senza RAPL leggibile** (modelli AMD, Intel più datati) non
  possono essere ordinate. I profili restano comunque elencati e selezionabili
  a mano, ma i quattro livelli deliberatamente lasciano stare il firmware
  invece di indovinare un ordine — nel firmware sopra, indovinare in base
  all'indice metterebbe Turbo sul profilo da 45 W.
- Il firmware **dimentica** il profilo a ogni ciclo di alimentazione, quindi
  il servizio di boot riapplica l'ultimo scelto.
- Sui modelli dove il firmware lega l'illuminazione della tastiera alla
  modalità di potenza, ogni cambio — incluso ogni passo di una calibrazione —
  ridisegna la tastiera. È il firmware a farlo, non questa app; se ti dà
  fastidio, riapplica i tuoi colori dalla pagina Illuminazione in seguito.
- Il **tasto fisico di cambio modalità** percorre lo stesso ordine misurato;
  vedi sotto.

### Tasto Fisico di Cambio Modalità

Alcuni modelli hanno un tasto dedicato che cicla tra le modalità di potenza.
Questo tasto si segnala **solo** come input report HID grezzo sull'embedded
controller e non genera alcun evento nel subsistema di input, ed è per questo
che appare morto su Linux mentre il tasto PredatorSense (un hotkey WMI)
funziona.

Il daemon osserva il dispositivo HID dell'EC Acer per intercettarlo. I valori
predefiniti sono stati catturati su un PHN16-73 (`1025:174B`, report
`04 85 ff`); ci si aspetta che altri modelli differiscano, quindi entrambi
sono sovrascrivibili senza ricompilare:

`~/.config/predator-sense/mode_key.json`:

```json
{ "product": "0000ABCD", "report": [4, 133, 255] }
```

(JSON rigoroso — un commento `//` in questo file lo rende non analizzabile, e
il daemon torna ai valori predefiniti registrando una nota nel suo log.)

Se il tuo tasto non fa nulla, il daemon registra nel log ogni dispositivo HID
Acer trovato all'avvio (attiva `debug_logging` in Impostazioni). Trova quello
giusto con `sudo hexdump -C /dev/hidrawN` mentre premi il tasto, poi punta il
file su di esso — e per favore apri una issue con i valori così possano
diventare i predefiniti per il tuo modello.

Il firmware inoltre rifiuta di cambiare modalità sotto il 40% di batteria; il
daemon lo segnala invece di far sembrare il tasto rotto.

### Profilo Automatico in base all'Alimentazione

Quando attivato in Impostazioni (attivo di default sulle nuove
installazioni), non è solo una reazione al collegare/scollegare
l'alimentazione - viene applicato continuamente:
- **Con alimentazione:** sempre Prestazioni o Turbo. Se uno dei due è già
  attivo, viene lasciato com'è - il cambio automatico non contrasta mai una
  scelta manuale tra i due.
- **A batteria:** sempre Bilanciato o Silenzioso, mai Prestazioni/Turbo. Sotto
  il 15% di batteria, Silenzioso viene forzato indipendentemente
  dall'obiettivo configurato.

### Dashboard GPU

Monitoraggio NVIDIA GPU in tempo reale:
- Temperatura, utilizzo, uso VRAM, consumo (gauge circolari)
- Grafici storici in tempo reale di temperatura e utilizzo (finestra di 2 minuti)
- Clock core, clock memoria, P-State, info link PCIe, versione VBIOS

### Assistente IA (beta)

Un assistente IA locale e opzionale, basato su [Ollama](https://ollama.com) in esecuzione interamente sulla tua macchina — nulla viene inviato altrove.

1. Installa Ollama separatamente seguendo le [istruzioni ufficiali per Linux](https://ollama.com/download/linux)
2. Vai su **AI** nella barra laterale e scarica un modello dal gestore modelli integrato (`smollm2:1.7b` o superiore — i modelli più piccoli non supportano in modo affidabile il tool-calling)
3. Attiva l'assistente in **Impostazioni** e scegli **Applica automaticamente** (applica subito i suggerimenti) o **Conferma sempre** (predefinito — ogni modifica suggerita attende la tua approvazione)

L'assistente legge lo stato dell'hardware in tempo reale (temperatura, ventole, profilo termico, batteria) e può suggerire o applicare modifiche tramite un insieme fisso di azioni già validate — non accede mai direttamente all'hardware/EC grezzo, e ogni azione corrisponde 1:1 a una funzione che questa app già usava prima che esistesse la funzionalità IA. Il modello viene caricato solo per eseguire un'analisi, poi viene scaricato — non resta inattivo in memoria. Tutta l'attività dell'IA viene registrata in un log delle azioni persistente e consultabile, nella stessa pagina.

---

## Opzioni dell'Installer

L'installer Rust offre una TUI interattiva:

```console
sudo ./predator-sense-installer              # Menu interattivo
sudo ./predator-sense-installer --install    # Installazione completa diretta
sudo ./predator-sense-installer --uninstall  # Rimuove tutto
sudo ./predator-sense-installer --reload-module # Ricompila/ricarica il modulo kernel
sudo ./predator-sense-installer --status     # Mostra lo stato dei componenti
```

---

## Disinstallazione

```console
sudo ./predator-sense-installer  # Seleziona l'opzione 2
```

Oppure manualmente:
```console
pkill -f "/opt/predator-sense/predator-sense"
sudo rm -rf /opt/predator-sense
sudo rm -f /usr/share/applications/predator-sense.desktop
sudo rm -f /usr/share/icons/hicolor/128x128/apps/predator-sense.png
rm -f ~/.config/systemd/user/predator-sense-hotkey.service
rm -f ~/.config/autostart/predator-sense-hotkey.desktop
sudo rmmod facer  # Opzionale: scarica il modulo kernel
```

---

## Risoluzione dei Problemi

<details>
<summary><b>L'RGB della tastiera non cambia / bloccato su un effetto</b></summary>

Lo stato del modulo kernel potrebbe essere bloccato. Ricaricalo:
```console
sudo rmmod facer
sudo insmod /path/to/kernel/facer.ko
# Oppure usa l'installer: sudo ./predator-sense-installer → Opzione 4
```
</details>

<details>
<summary><b>Il modulo non si carica</b></summary>

```console
# Verifica che il dispositivo WMI esista
ls /sys/bus/wmi/devices/7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56/

# Controlla i log del kernel
sudo dmesg | grep -i facer

# Assicurati che gli header corrispondano al tuo kernel
sudo apt install linux-headers-$(uname -r)
```
</details>

<details>
<summary><b>Il tasto PredatorSense non funziona</b></summary>

```console
# Verifica il servizio hotkey Rust
systemctl --user status predator-sense-hotkey.service
pgrep -af predator-sense-hotkey

# Assicurati che l'utente sia nel gruppo 'input' (serve logout/login completo o riavvio dopo l'aggiunta)
groups | grep input
sudo usermod -aG input $USER
```
</details>

<details>
<summary><b>La pagina GPU NVIDIA non mostra dati</b></summary>

```console
# Verifica che nvidia-smi funzioni
nvidia-smi
# In caso contrario, installa i driver proprietari NVIDIA
```
</details>

<details>
<summary><b>Il mio modello non ha un quirk corrispondente (mancano profili/lettura ventole/PWM)</b></summary>

Se il tuo modello esatto non è ancora nella lista di compatibilità, prova a forzare tutte le funzionalità opzionali della famiglia `predator_v4` e vedi cosa funziona davvero sul tuo hardware:

```console
sudo modprobe facer enable_all=1
# persistente tra i riavvii:
echo "options facer enable_all=1" | sudo tee /etc/modprobe.d/facer-options.conf
```

È solo WMI (nessuna scrittura raw sull'EC), quindi su hardware che non implementa una determinata funzionalità è un no-op sicuro, non una scrittura dannosa. Per favore [apri una issue](https://github.com/cleyton1986/predator-sense/issues) con il tuo modello e cosa ha funzionato/non ha funzionato — è così che vengono aggiunti nuovi quirk.
</details>

---

## Struttura del Progetto

```
predator-sense-gui/
├── kernel/                      # Moduli del kernel Linux (gestiti da DKMS)
│   ├── facer.c                  # Interfaccia ACPI/WMI verso l'hardware Acer
│   ├── acer-wmi-battery.c       # Supporto al limite di carica della batteria
│   ├── acpi_ec.c                # Accesso raw all'EC via /dev/ec (da MusiKid/acpi_ec)
│   ├── Makefile
│   └── dkms.conf                # Configurazione DKMS per la ricompilazione automatica
├── installer/                   # Installer multicall e servizi Rust
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # Dispatch tipizzato in base al nome dell'eseguibile installato
│       ├── constants.rs         # Percorsi centrali, valori di protocollo e costanti hardware
│       ├── install.rs           # Installer + registrazione DKMS
│       ├── helper.rs            # Operazioni hardware privilegiate validate
│       ├── hotkey.rs            # Listener di eventi input Linux
│       ├── tray.rs              # Servizio StatusNotifierItem
│       └── i18n.rs              # Messaggi tipizzati EN/PT
├── protocol/                    # Contratto tipizzato condiviso tra GUI e helper
│   ├── Cargo.toml
│   └── src/lib.rs               # Azioni, percorsi, limiti e nomi dei binari
├── src/                         # Applicazione Rust GTK4
│   ├── main.rs
│   ├── app_state.rs             # Flag globale di visibilità della finestra (blocca i timer)
│   ├── i18n.rs                  # Internazionalizzazione EN/PT
│   ├── config.rs                # Preferenze utente (JSON)
│   ├── tray.rs                  # Ciclo di vita del servizio tray Rust
│   ├── hardware/
│   │   ├── helper.rs            # Client tipizzato dell'helper privilegiato
│   │   ├── rgb.rs               # RGB via /dev/acer-gkbbl-*
│   │   ├── hwmon.rs             # Indice /sys/class/hwmon (cache in OnceLock)
│   │   ├── sensors.rs           # Temperature, ventole, RAM, rete
│   │   ├── gpu.rs               # Parser di nvidia-smi con cache TTL
│   │   ├── procs.rs             # Sampler /proc (CPU per core, memoria, elenco processi)
│   │   ├── storage.rs           # Utilizzo disco via df
│   │   ├── sysinfo.rs           # Specifiche DMI + CPU + GPU + SO
│   │   ├── fan.rs               # Modalità ventole + CoolBoost
│   │   ├── extras.rs            # Limite batteria, LCD overdrive, ricarica USB, animazione di boot
│   │   ├── profile.rs           # CPU governor + EPP + potenza GPU
│   │   ├── ai_assistant.rs      # Tool-calling di Ollama: allow-list fissa mappata sui setter hardware:: già esistenti
│   │   ├── ai_snapshot.rs       # Snapshot effimero dello stato hardware, fornito all'IA e cancellato a ogni lettura
│   │   ├── ai_actionlog.rs      # Log persistente e consultabile di tutto ciò che l'IA ha suggerito/applicato
│   │   └── setup.rs             # Gestione del modulo kernel
│   └── ui/                      # Pagine GTK4 (widget Cairo personalizzati)
│       ├── window.rs            # Finestra principale, sidebar, barre neon, riduci a tray
│       ├── dashboard_page.rs    # Hero + specifiche di sistema
│       ├── temperatures_page.rs # Tutti i gauge di temperatura
│       ├── usage_page.rs        # CPU/GPU/Mem/Storage con i processi principali
│       ├── network_page.rs      # Download/upload con tracciamento dei picchi
│       ├── rgb_page.rs          # RGB della tastiera con zone visive
│       ├── fan_control_page.rs  # Ventole animate + CoolBoost
│       ├── fan_page.rs          # Profili di prestazioni
│       ├── battery_page.rs      # Statistiche batteria + limite di carica
│       ├── gpu_page.rs          # Dashboard NVIDIA GPU
│       ├── monitor_page.rs      # Grafici storici dettagliati CPU/GPU
│       ├── ai_page.rs           # Assistente IA: chat, gestione modelli, monitor risorse, log delle azioni
│       ├── setup_page.rs        # Wizard di configurazione del modulo kernel
│       └── gauge_widget.rs      # Widget gauge circolare tratteggiato
└── resources/
    ├── style.css                # Tema scuro gaming
    └── predator-icon.svg        # Icona del tray di sistema
```

---

## Crediti e Ringraziamenti

- **Modulo kernel `facer`** basato sul progetto [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) di [JafarAkhondali](https://github.com/JafarAkhondali) e [tutti i collaboratori](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module/graphs/contributors)
- **Modulo kernel `acpi_ec`** di [Sayafdine Said (MusiKid)](https://github.com/MusiKid/acpi_ec) — espone `/dev/ec` per la lettura/scrittura raw dell'EC. Usato dall'helper per impostare modalità ventole, CoolBoost, LCD overdrive, ricarica USB e animazione di boot.
- **Applicazione GUI** sviluppata con [Rust](https://www.rust-lang.org/) + [GTK4](https://gtk.org/) + [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/)
- **Installer e servizi in background** sviluppati con [Rust](https://www.rust-lang.org/); l'integrazione con il tray usa [ksni](https://crates.io/crates/ksni)
- **Icone di Dashboard e Temperature** (`predator-sense-gui/resources/icons/`) da [Flaticon](https://www.flaticon.com), create da Hilmy Abiyyu A., magnific e mehwish

### Fare Fork o Riutilizzare questo Progetto

Questo progetto è concesso in licenza sotto GPL-3.0, quindi sei libero di fare fork, modificarlo e ridistribuirlo sotto la stessa licenza. Se lo fai — soprattutto se costruisci un'app derivata o riutilizzi parti significative della GUI/modulo kernel — **per favore mantieni un credito visibile all'autore originale** (basta una menzione a [Cleyton Alves](https://github.com/cleyton1986) / questo repository nel tuo README, nella schermata Informazioni o nella sezione crediti). È una piccola richiesta che fa una grande differenza per un progetto collaterale indipendente e non retribuito.

## Sostieni il Progetto

Se questo progetto ti è stato utile e vuoi supportarne lo sviluppo, considera di offrirmi un caffè:

<p align="center">
  <a href="https://www.paypal.com/cgi-bin/webscr?cmd=_donations&business=cleyton1986%40gmail.com&currency_code=BRL&item_name=Predator+Sense+for+Linux">
    <img src="https://img.shields.io/badge/PayPal-Donate-00457C?logo=paypal&logoColor=white&style=for-the-badge" alt="Donate via PayPal">
  </a>
</p>

<p align="center">
  <b>PIX (Brasile):</b> <code>cleyton1986@gmail.com</code>
</p>

Qualsiasi contributo è volontario ed è molto apprezzato! Aiuta a mantenere vivo il progetto e motiva nuove funzionalità.

---

## Licenza

Questo progetto è concesso in licenza sotto la **GNU General Public License v3.0** — vedi il file [LICENSE](LICENSE) per i dettagli.

Questo è software libero: puoi ridistribuirlo e/o modificarlo secondo i termini della GNU GPL come pubblicata dalla Free Software Foundation.

**Eccezione — immagini dei prodotti:** la licenza GPLv3 sopra copre solo il codice sorgente di questo progetto. Le foto dei notebook Acer Predator/Nitro in `predator-sense-gui/resources/models/` sono immagini di prodotti di terze parti (vedi [Avviso legale](#avviso-legale) sopra) e **non** sono coperte dalla concessione GPLv3; tutti i diritti su queste immagini restano di Acer Inc. e/o dei fotografi originali.

**Questo software è fornito "così com'è", senza garanzie di alcun tipo.** Gli autori non sono responsabili per eventuali danni che possano derivare dall'uso di questo software. Installando e utilizzando questo software, riconosci di farlo a tuo rischio e pericolo.
