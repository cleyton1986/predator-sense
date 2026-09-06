# Predator Sense for Linux (日本語)

<p align="center">
  <a href="README.md">🇺🇸 Read in English</a> · <a href="README-ptbr.md">🇧🇷 Leia em Português</a> · <a href="README-es.md">🇪🇸 Leer en Español</a> · <a href="README-zh.md">🇨🇳 阅读中文版</a> · <a href="README-ru.md">🇷🇺 Читать на русском</a> · <a href="README-de.md">🇩🇪 Auf Deutsch lesen</a> · <a href="README-it.md">🇮🇹 Leggi in Italiano</a> · <a href="README-tr.md">🇹🇷 Türkçe Oku</a>
</p>

<p align="center">
  <img src="predator-sense-gui/resources/logo.jpeg" width="120" alt="Predator Sense Logo">
</p>

<p align="center">
  <b>Acerゲーミングノートパソコンのハードウェア制御用の非公式Linuxカーネルモジュール兼GUI</b><br>
  <i>RGBキーボードバックライト &bull; ターボモード &bull; 温度モニタリング &bull; パフォーマンスプロファイル</i>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Language-Rust-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/GTK-4-blue?logo=gtk" alt="GTK4">
  <img src="https://img.shields.io/badge/Userspace-100%25_Rust-orange?logo=rust" alt="100% Rust userspace">
  <img src="https://img.shields.io/badge/License-GPL--3.0-green" alt="License">
  <img src="https://img.shields.io/badge/Platform-Linux-yellow?logo=linux" alt="Linux">
</p>

<p align="center">
  作成・保守: <a href="https://github.com/cleyton1986">Cleyton Alves</a>
</p>

---

## 免責事項

> **警告**
> **利用は自己責任で行ってください！** これは**非公式**のプロジェクトです。Acerはその開発に一切関与していません。このカーネルモジュールは、公式のPredatorSense Windowsアプリケーションをリバースエンジニアリングして開発されました。このドライバーは、すべてのノートパソコンシリーズでテストされていない低レベルのWMI/ACPIメソッドとやり取りします。作者は、お使いのハードウェアに生じるいかなる損害についても責任を負いません。

> **注記**
> 記載されているすべての商標、製品名、ロゴ（Acer、Predator、PredatorSense、Helios、Nitro、AeroBlade、CoolBoost）は、それぞれの所有者（Acer Inc.）の財産です。このプロジェクトは、いかなる形でもAcer Inc.と提携、承認、後援されているものではありません。

> **製品画像について**
> `predator-sense-gui/resources/models/`にあるノートパソコンの写真は、Acer Predator/Nitroの正規製品を写したものであり、ユーザー自身のマシンで検出された機種を（システムのDMI/BIOSが報告する`product_name`と照合して）アプリが視覚的に識別できるようにするためだけに使用されています。これらの画像は**このプロジェクトのGPLv3ライセンスの対象ではありません**。製品写真そのものの著作権はAcer Inc.および/または元の制作者に帰属します。これらの画像は、善意に基づき、非営利かつ純粋に情報提供の目的（名称的/製品識別のための利用）で、このプロジェクトによる所有権の主張なしに掲載されています。あなたが権利者であり、画像の削除を希望する場合は、issueを開いてください。速やかに削除します。

このアプリケーションは、AcerがLinux向けにPredatorSenseの公式サポートを提供していないことから、Acerゲーミングノートパソコンをlinux上で最大限に活用するために、**個人利用**を目的として作成されました。同じことを望む誰もが自由に使えるように公開しています。

このアプリ/プロジェクトが役に立った、あるいは何らかの形で気に入っていただけたなら、スターを付けていただけると大変励みになります ⭐

---

## スクリーンショット

<p align="center"><b>Dashboard</b> — ノートパソコンの写真と、CPU、GPU、RAM、ストレージ、ネットワーク、OSなど、システムの全仕様を一目で確認できます。</p>
<p align="center"><img src="assets/psense-1.png" width="800" alt="Dashboard"></p>

<p align="center"><b>温度</b> — CPU、GPU、システム、NVMeドライブ、WiFi、RAMのライブゲージを1画面にまとめて表示します。</p>
<p align="center"><img src="assets/psense-2.png" width="800" alt="Temperatures"></p>

<p align="center"><b>使用率</b> — CPU、GPU、メモリ、ストレージについて、上位プロセス、アニメーション付きバー、クリックで展開できる詳細を表示します（温度ゲージにはCSSベースの炎アニメーションも付いています）。</p>
<p align="center"><img src="assets/psense-3.png" width="800" alt="Usage"></p>

<p align="center"><b>ネットワーク</b> — ピーク値の追跡と自動インターフェース検出（Wi-FiまたはEthernet）を備えたリアルタイムのダウンロード/アップロードグラフです。</p>
<p align="center"><img src="assets/psense-4.png" width="800" alt="Network"></p>

<p align="center"><b>ライティング</b> — ゾーンごとのスタティック設定（4セクション）と、動的なRGBキーボードエフェクト（Breathing、Neon、Wave、Shifting、Zoom）です。</p>
<p align="center"><img src="assets/psense-5.png" width="800" alt="Lighting"></p>

<p align="center"><b>モード</b> — パフォーマンスプロファイル：Quiet、Balanced、Performance、Turbo、さらにバッテリー駆動時専用のEco階層（CPUガバナー + Intel EPP + GPU電力制限）です。</p>
<p align="center"><img src="assets/psense-6.png" width="800" alt="Modes"></p>

<p align="center"><b>GameSync</b> — ゲームとそのプロファイルを登録すると、ゲームの実行中はアプリが自動的にそのプロファイルへ切り替え、終了すると直前まで有効だったプロファイルに復元します。</p>
<p align="center"><img src="assets/psense-15.png" width="800" alt="GameSync"></p>

<p align="center"><b>ファン制御</b> — アニメーションで回転するファンによるライブRPM表示、CoolBoostの切り替え、Auto/Maxモードです。</p>
<p align="center"><img src="assets/psense-7.png" width="800" alt="Fan Control"></p>

<p align="center"><b>バッテリー</b> — 充電率、電圧、電流、電力、サイクル数、劣化状態、製造元、そして長寿命化のための80%充電制限です。</p>
<p align="center"><img src="assets/psense-8.png" width="800" alt="Battery"></p>

<p align="center"><b>GPU</b> — ライブグラフ、クロック、使用率、VRAM、消費電力、PCIe情報を備えたNVIDIAダッシュボードです。</p>
<p align="center"><img src="assets/psense-9.png" width="800" alt="GPU"></p>

<p align="center"><b>グラフ</b> — 最小値/最大値の追跡を備えた、詳細なCPUおよびGPUの履歴チャートです。</p>
<p align="center"><img src="assets/psense-10.png" width="800" alt="Graphs"></p>

<p align="center"><b>AIアシスタント（ベータ版）</b> — Ollamaを利用したローカルAIアシスタント：チャット、モデルマネージャー（インストール済みモデルの一覧表示、新規モデルのダウンロード、実行するモデルの選択）、思考中のライブVRAM/GPUリソース使用状況、そして永続的なアクションログです。</p>
<p align="center"><img src="assets/psense-11.png" width="800" alt="AI Assistant"></p>

<p align="center"><b>ドライバーとマニュアル</b> — シリアル番号（コピー用ボタン付き）と、Acer公式のドライバー・マニュアルページへの直接リンクを表示し、さらにノートパソコン上のシリアル番号ステッカーの場所を示す図も表示します。</p>
<p align="center"><img src="assets/psense-16.png" width="800" alt="Drivers and manuals"></p>

<p align="center"><b>設定</b> — トレイへの最小化、起動時の自動実行、起動時のプロファイル自動適用、言語設定、機種ごとの対応機能一覧です。</p>
<p align="center"><img src="assets/psense-12.png" width="800" alt="Settings"></p>

<p align="center"><b>カバーロゴのライティング</b> — カラー対応のカバーロゴを備えた機種において、ディスプレイ背面のロゴを独立して制御するRGB機能です（Static/Breathing/Neon）。実行時に検出されます。コントロールは、ハードウェアが機能問い合わせに応答した場合にのみ表示され、対応していない機種では安全に非表示のままになります。</p>
<p align="center"><img src="assets/psense-13.png" width="800" alt="Cover logo lighting"></p>
<p align="center"><img src="assets/psense-14.jpg" width="800" alt="Cover logo lit up green on a Predator PHN16-73"></p>
<p align="center"><sub>この機能は<a href="https://github.com/jlucaso1">@jlucaso1</a>氏の貢献によるもので、氏自身のPredator PHN16-73で検証されました。このノートパソコンのカバーロゴはカラー非対応ですが、氏のハードウェアを使って機能そのものの動作は確認されています。</sub></p>

---

## 概要

Acerゲーミングノートパソコン（Acer Predator、Acer Helios、Acer Nitro）のRGBキーボードバックライトおよびターボモード用の非公式Linuxカーネルモジュールです。

[JafarAkhondali](https://github.com/JafarAkhondali)氏とその貢献者による[acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module)プロジェクトに着想を得て、それをベースにしています。このプロジェクトは、既存のLinux Acer-WMIカーネルモジュールを拡張してAcerのゲーミング機能をサポートし、さらにRustとGTK4で構築された**フル機能のGUIデスクトップアプリケーション**を追加します。

---

## 機能

| 機能 | 説明 |
|---------|-------------|
| **Dashboard** | ノートパソコンの写真 + 完全なシステム仕様（CPU、GPU、RAM、ストレージ、ネットワーク、OS） |
| **温度** | CPU、GPU、システム、NVMe、WiFi、RAMのライブゲージ |
| **使用率** | 4タブ表示：CPU / GPU / メモリ / ストレージ、上位プロセス、クリックで展開できる詳細、温度ゲージのCSSベース炎アニメーション付き |
| **ネットワーク** | ピーク値の追跡と自動インターフェース検出を備えたリアルタイムのダウンロード/アップロードグラフ |
| **RGBキーボード制御** | WMI経由での、ゾーンごとのスタティック設定（4ゾーン）と動的エフェクト（Breathing、Neon、Wave、Shifting、Zoom）。カーネルモジュールのないハードウェアでは、代わりにRGBがUSB/I2C-HID経由でネイティブに動作します — ENEK5130チップ（4ゾーンスタティック、Breathing/Neon）、2024年以降のSunrexチップ（単一ゾーン、全エフェクト対応）、またはChiconyチップ（7色パレット、Helios 300）— 自動検出されます。[互換性](#互換性)を参照してください |
| **RGBカバーロゴ** | ディスプレイ背面のエンブレム用の、独立した電源、単色、輝度、Breathing、Neonの各制御。ライブベクタープレビュー付き。実行時のHID機能検出後にのみ表示されます |
| **パフォーマンスプロファイル** | Quiet / Balanced / Performance / Turboの各モードに加え、バッテリー駆動時専用のEco階層（CPUガバナー + Intel EPP + GPU電力制限） |
| **ファン制御** | アニメーションで回転するファンによるライブRPM表示、CoolBoostの切り替え、Auto/Maxモード、さらに（対応している場合は）実験的なファンごとのPWM制御と自動温度カーブ |
| **バッテリー** | 充電統計、サイクル数、劣化状態、製造元情報、そして長寿命化のための80%充電制限 |
| **GPUダッシュボード** | NVIDIAの各種メトリクス：温度、使用率、VRAM、クロック、消費電力、PCIe情報をライブグラフで表示、さらに**電力制限（TGP）スライダー** |
| **グラフ** | 最小値/最大値の追跡を備えた、詳細なCPUおよびGPUの履歴チャート |
| **AIアシスタント** 🧪 | [Ollama](https://ollama.com)を利用した、オプトイン方式のローカルAIアシスタント — ライブのハードウェア状態を読み取り、固定された検証済みのアクション集合（サーマルプロファイル、ファンモード、CoolBoost、RGB、GPU電力制限、バッテリー）を通じて変更を提案または適用します。チャット、モデルマネージャー（ダウンロード/選択）、ライブのリソース/VRAMモニター、永続的なアクションログを備えています。自動適用にするか、常に確認を求めるかは選択できます。Ollamaを別途インストールする必要があります — 詳しくは下記の[AIアシスタント（ベータ版）](#aiアシスタントベータ版)を参照してください |
| **自動機能検出** | 各機種がサポートする機能を検出してUIを適応させます — 未対応の機能はエラーではなく「この機種では利用できません」と表示されます。対応している機能は設定画面に一覧表示されます |
| **温度アラート** | CPU/GPUが90°Cを超えるとデスクトップ通知を表示します（トレイに格納中でも動作します） |
| **自動電源プロファイル** | AC/バッテリーの切り替え時にプロファイルを自動的に切り替えます — 各状態のターゲットプロファイルは設定画面で設定可能です（デフォルト：AC電源時はPerformance、バッテリー駆動時はBalanced） |
| **デバッグログ** | 設定画面のオプションのトグル — デーモンとアプリのイベントを`~/.local/share/predator-sense/`にログとして記録します（ローテーション、5MB×3）。リモートでのトラブルシューティングに使用します。デフォルトはオフです |
| **システムトレイ** | Predatorアイコンでトレイに最小化 — アプリはバックグラウンドで動作し続けます |
| **PredatorSenseキー** | ハードウェアキーのマッピング — NumLockの隣にあるキーでアプリを開きます |
| **DKMS** | カーネルアップグレード時にカーネルモジュールが自動的に再ビルドされます |
| **国際化** | システムのロケールに応じて英語/ポルトガル語を自動切り替え |
| **ゲーミングUI** | ネオン風に脈動するバー、破線の円形ゲージ、ポリゴン状のパネル枠を備えたダークテーマ。アクセントカラーは検出されたブランドに応じて自動的に切り替わります — Predator/Helios/Tritonではシアン、Nitroではオレンジ/レッド（NitroSenseに準拠）— 切り替えるための設定項目はありません |

---

## 互換性

**自分のノートパソコンで動作しますか？**

凡例：✅ テスト済み・動作確認済み · 🟡 実装済み、未テスト（テスターを募集中） · 🧪 実験的（テスターを募集中） · ❌ 動作しない · `-` 該当なし

| Product Name | Turbo (Impl.) | Turbo (Tested) | RGB (Impl.) | RGB (Tested) | Fan RPM read | Fan profiles | Fan PWM % |
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

> お使いの機種が表に載っていない場合でも動作する可能性があります — カーネルモジュールは対応するWMIインターフェースを自動的に検出します。動作した（あるいは動作しなかった）場合は、機種名を明記してissueを開いていただければ、この表を更新できます。

### ファン制御 — 3つのレベル

| レベル | 内容 | 対応状況 |
|---|---|---|
| **ファンRPM読み取り** | CPU/GPUファンの回転数を読み取る（`fan1_input`、`fan2_input`） | ほとんどのゲーミング機種（自動検出） |
| **ファンプロファイル** | `platform_profile`によるQuiet / Balanced / Performance / Turbo | `predator_v4`系の機種 |
| **ファンPWM %** 🧪 | ファンごとの速度制御（`pwm1`/`pwm2` 0～100%）。mainlineの`acer-wmi`からWMI経由で移植 — **カーネル6.14以上限定** | `ACER_CAP_PWM`を備えた一部の機種（AN515-58、PHN16-72/73、PH16-72、…） |

> **🧪 PWMファン制御は実験的機能です。** これは上流のLinuxカーネル`acer-wmi`ドライバーから移植されたもので、安全なWMIメソッドを使用しています（ EC への生の書き込みは行いません）が、メンテナー自身の**実機では検証されていません**（メンテナーが所有しているPH315-54にはPWMがないため）。対応機種をお持ちの場合は、テスト結果の報告を歓迎します。**利用は自己責任で行ってください** — 冒頭の免責事項を参照してください。

### 代替手段：linuwu_sense（quirkテーブルに未登録で、Turboが動作しないハードウェア向け）

`facer`の`enable_all=1`フォールバックは、Acer WMI対応のボードであれば何でも認識しますが、完全な`predator_v4`プロファイル群（書き込み可能な`turbo_state`を含む、`balanced-performance`/`performance`を含む5つのプロファイル）は、独自のDMI quirkテーブルに登録されているボードにのみ適用されます。quirkテーブルに未登録のボードでは、`platform_profile_choices`が`low-power quiet balanced`に限定され、ファームウェアがそれ以上をサポートしていても`turbo_state`は読み取り専用のままです。これは、あるPHN16-73機（Macan_ARX、BIOS V1.26）で報告されており、issue [#33](https://github.com/cleyton1986/predator-sense/issues/33)で確認できます。

これに該当する場合、コミュニティ製の[Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense)モジュール（`predator_v4=1`で読み込む）は、このアプリがすでに直接読み取っているのと同じ汎用の`platform_profile`/`intel_pstate`/`acer-wmi-battery`インターフェースを通じて、完全なプロファイル群を公開します — `facer`固有のコードパスは一切関与しません。`v0.2.71-preview`以降、アプリは`linuwu_sense`を検出し、実際にこれらのインターフェースを提供しているのがそのドライバーである場合は「facerをインストールしてください」というプロンプトをスキップします。RGBおよびサーマルプロファイルのキャリブレーション（どちらも`facer`専用の機能、上記および下記を参照）は、linuwu_sense環境下では引き続き利用できません。

### カーネルモジュールなしでのRGB（I2C-HIDハードウェアのみ）

一部の機種（確認済み：PHN16S-71、PHN16-73、AN16S-61）では、キーボードのRGBコントローラーが`facer.ko`のWMIインターフェースではなく、独立したI2C-HIDチップ（ENEK5130）を経由しています — アプリはこれと`/dev/hidrawN`を通じて直接通信するため、カーネルモジュールがまったく読み込まれていなくてもこれらは動作します。

| 機能 | 状態 |
|---|---|
| ゾーンごとのスタティックカラー、輝度、バックライトオフ | ✅ 動作確認済み（PHN16S-71、AN16S-61） |
| 動的エフェクト — Breathing、Neon | ✅ 動作確認済み（PHN16S-71、AN16S-61）— ネイティブ動作で、単一のHID書き込みでハードウェアがパターンを自律的にループ再生します。PHN16S-71機ではBreathingが選択した色を無視し、代わりにレインボーサイクルになります。他のハードウェアでは異なる可能性があります |
| 動的エフェクト — Wave、Shifting、Zoom | 画面上のプレビューのみ（ハードウェアへの書き込みなし）— これらのエフェクトコードが、ハードウェア世代ごとに異なる意味を持つことが判明したため、まだ実装されていません |
| RGBカバーロゴ — オフ、単色、輝度、Breathing、Neon | ✅ 動作確認済み（PHN16-73） |

カバーロゴのサポートは、機種名によるアローリストで有効化されているわけではありません。UIが表示される前に、コントローラーがA1ターゲットレポートでターゲット`0x83`を通知し、それに対応する空でないA3ケーパビリティを返す必要があります。アプリは書き込みのたびに、この確認をその都度実行します。ホットキーデーモンは、ログイン後およびレジューム後に、アプリが以前に正常に適用した設定のみを復元し、保存された設定がない場合やターゲットが存在しない場合はロゴの処理を完全にスキップします。

[AN16S-61に関する独立したレポート](https://github.com/cleyton1986/predator-sense/issues/31)（報告者自身による[スタンドアロンのプロトコルツール](https://github.com/ArnarValur/Nitro16S-AI-RGB-Keyboard)も参照）では、static/Breathing/Neon/Wave以外に、さらに6つのネイティブなワイヤーモード（ハードウェアのオフモード、EC自体がトリガーする起動時の点滅モード、その他4つの組み込みアニメーション）と、モード/ターボキーのLEDターゲットがマッピングされています。これらはまだアプリに組み込まれていません — 組み込むには、ハードウェアネイティブなエフェクトコード用の定義済みスロットが必要なため、今後の改善項目として管理されています。

同じレポートには、コントローラーから直接取得された、デコード済みのHIDレポートディスクリプタも含まれており、これによって実際のバグが1つ判明しました：アプリはA3ケーパビリティレポートのゾーン数を、誤ったバイト（`byte[3]`、ターゲットクラスごとの固定定数）から読み取っていましたが、正しくはコントローラー自身のディスクリプタがこの値のために宣言しているバイト（`byte[4]`）を読み取るべきでした。これはアプリとホットキーデーモンの両方で`v0.2.69-preview`にて修正されました。これは機種ごとの変更ではなく、プロトコルレベルの修正です — レポートディスクリプタのフィールドレイアウトはチップ自体のファームウェアに由来し（確認済みの3機種すべてで同じ`0CF2:5130`チップ）、以前の値は常に正しい値を包含する上位互換の集合だったため、すでに動作確認済みのハードウェアでワイヤー上のバイトが変わることはありません。

### 2024年以降のハードウェアでのRGB（Sunrex/Darfon USB HID）

新しい世代の機種（PH16-72、および同じUSB HIDチップを共有する他の2024～2026年モデル。issue #26を参照）では、キーボードおよびカバーロゴのRGBが、WMIからも上記のENEK5130チップからも完全に切り離され、まったく別の2つのコントローラーへ移行しました — キーボード用のSunrex `05af:*`と、ロゴ用のDarfon `0d62:*`です。アプリはこれらも直接検出・制御し、存在する場合はENEK5130/WMI経路よりも自動的に優先して選択されます。

| 機能 | 状態 |
|---|---|
| キーボード：Off、Static、Breathing、Wave、Snake、Neon、Spot、Star、Rainbow、5× Slash、Zoom、Row Wave、Swiping | 🟡 実装済み、実機での確認待ち |
| カバーロゴ：off、単色、輝度、Breathing | 🟡 実装済み、実機での確認待ち |

このチップには独立したゾーンがありません — 上記の4ゾーンENEK5130コントローラーとは異なり、キーボード全体が一度に1つの色/エフェクトになります。ワイヤープロトコルは、公式Windowsアプリの2つの逆コンパイル済みリリースから、1バイト単位でリバースエンジニアリングされたものです（固定バイト列とチェックサム計算式がすべて両者で完全に一致しました）。推測ではありませんが、まだ誰も物理的なハードウェアで確認していないため、実際のレポートが届くまでは未テストとして扱ってください。

3つ目のチップ（Chicony、Helios 300/PH317-56）は、コミュニティのリバースエンジニアリングによって文書化された（[NT411/Acer-Predator-Fan-RGB-Controller-Linux](https://github.com/NT411/Acer-Predator-Fan-RGB-Controller-Linux)）さらに別のUSB HIDプロトコルを使用しており、その仕様をもとにここで再実装されています — 12種類のエフェクトにわたる固定7色パレット（任意のRGBではなく、ハードウェア/ファームウェアの制約によるものです）。こちらも🟡で、確認待ちです。

### すでにLinuwu-SenseやDAMXを使っていますか？

[Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense)（およびそれをベースに構築された[DAMX](https://github.com/PXDiv/Div-Acer-Manager-Max)）は、LinuxでAcer Predator/Nitroのハードウェアを制御する、別の無関係なプロジェクトです。このプロジェクトの依存関係ではなく、そのコードもここでは一切使用していません — しかし、そのカーネルモジュールは`facer`が必要とするのと**同じWMI GUID**にバインドするため、カーネルは同じデバイスを2つのドライバーが同時に占有することを許可しません。

インストーラーは、`linuwu_sense`がすでに読み込まれている、またはDKMSでインストール済みであることを検出すると、自動的に**既存のセットアップに一切手を加えません** — `acer_wmi`をブラックリストに入れることも、`facer`を強制的に読み込むこともしないため、すでに動作しているLinuwu-Sense/DAMXのインストールと衝突したり（あるいはそれを壊したり）することはありません。どのプラットフォームドライバーが有効であっても、キーボードRGBはこのアプリを通じてHID経路（上記参照）で引き続き動作します。この場合、ファン/サーマル制御は、すでにお使いの管理ツールに委ねられたままになります。

---

## インストール

### ビルド済みインストーラー（最も速い方法）

リリースインストーラーを直接ダウンロードして実行します。

```console
curl --fail --location https://github.com/cleyton1986/predator-sense/releases/latest/download/predator-sense-installer --output predator-sense-installer
chmod +x predator-sense-installer
sudo ./predator-sense-installer --install
```

インストーラー、特権ヘルパー、ホットキーリスナー、トレイサービスはすべて、同じRustマルチコールバイナリによって提供されます。インストーラーは、シェルスクリプトによるブートストラップなしにすべてをダウンロード・設定します。

### 対話型インストーラー（ビルド済みバイナリ、Rustツールチェーン不要）

[Releases](../../releases)ページから`predator-sense-installer`バイナリをダウンロードしてください。これは単体のRustバイナリであり、バンドルではありません — カーネルモジュール用のアプリのソースコードと、対応するビルド済みリリースバイナリを取得するためにインターネットへのアクセスは必要ですが、Rustのインストールとマシン上でのGTK4アプリのコンパイルは完全に不要です。

```console
chmod +x predator-sense-installer
sudo ./predator-sense-installer
```

**オプション1**（完全インストール）を選択してください。インストーラーは自動的に以下を行います。

1. ディストリビューションを検出（Debian/Ubuntu/Mint、Fedora、Arch）
2. システムの依存関係をインストール（GTK4、libadwaita、ビルドツール、カーネルヘッダー）
3. 対応するリリースのソースコード + ビルド済みバイナリをダウンロード
4. `facer`カーネルモジュールをコンパイルして読み込み（この部分は常にローカルでコンパイルされます — カーネルモジュールは異なるカーネルバージョン間でビルド済みとして配布できないためです）
5. アイコン付きのデスクトップメニュー項目を作成
6. PredatorSenseハードウェアキーをマッピング（ログイン時に自動起動）
7. システムトレイのサポートを設定

ビルド済みバイナリを使う方法では、対象マシンにRust/cargoは不要です。インストーラーは、ステータス確認、カーネルモジュールの再読み込み、アップグレード、アンインストール用のスタンドアロン管理ツールとして、`/opt/predator-sense/`にもコピーされます（[インストーラーのオプション](#インストーラーのオプション)を参照）。

インストール後、以下のいずれかの方法でアプリを開けます。
- **PredatorSenseキー**（NumLockの隣）を押す
- アプリケーションメニューで**「Predator Sense」**を検索する
- ターミナルで`/opt/predator-sense/predator-sense`を実行する

### 手動インストール（ソースからビルド）

#### 前提条件

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

**Rust**（未インストールの場合）：
```console
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### ビルドとインストール

```console
# リポジトリをクローン
git clone https://github.com/cleyton1986/predator-sense.git
cd predator-sense/predator-sense-gui

# GUIとRust製インストーラー/サービスをビルド
cargo build --release
cargo build --release --manifest-path installer/Cargo.toml

# ローカルビルドをインストールし、既存のC言語カーネルソースをDKMSに登録
sudo installer/target/release/predator-sense-installer --install

# 実行
/opt/predator-sense/predator-sense
```

---

## 使い方

### キーボードRGB

1. サイドバーの**ライティング**に移動します
2. **Static**（ゾーンごとの色）または**Dynamic**（エフェクト）を選択します
3. **Staticモード：** キーボードの4つのセクションごとにR/G/Bスライダーを調整します
4. **Dynamicモード：** エフェクト（Breathing、Neon、Wave、Shifting、Zoom）を選択し、速度を調整します
5. **適用**をクリックします

> カーネルモジュールを持たないI2C-HID専用のハードウェア（[互換性](#互換性)を参照）では、BreathingとNeonは実際にアニメーションします。Wave/Shifting/Zoomは画面上のプレビューのみで、その旨が明確に表示されます — これらについては、まだ物理的なキーボードは変化しません。

### RGBカバーロゴ

1. **ライティング**に移動し、**カバーロゴ**を選択します（このセレクターは、対応するHIDターゲットが検出された場合にのみ表示されます）
2. **ライティング**を使ってエンブレムのオン/オフを切り替えます
3. **Static**、**Breathing**、**Neon**のいずれかを選び、ライブプレビューを確認しながら、利用可能な色、輝度、速度の各コントロールを調整します
4. **ロゴに適用**をクリックします

最後に正常に適用された状態は、ユーザーホットキーサービスの起動時、およびサスペンド/ハイバネーションからの復帰後に復元されます。アニメーションエフェクトの色はファームウェアによって制御されるため、プレビューはこれらのモードについてカラーピッカーを提供するのではなく、その挙動を表現します。

> Linuxがユーザーサービスを起動する前に表示されるライティングアニメーションは、ファームウェアが管理しています。保存された「オフ」状態はログイン後に復元されますが、このアプリはそれ以前のBIOS/起動時アニメーションを抑制することはできません。

### パフォーマンスプロファイル

Intel P-State + HWPが有効なシステムでは、CPU側は以下のように決定されます。

| プロファイル | HWPポリシー | Intel EPP | 最小パフォーマンス | GPU電力 | ファン | 用途 |
|---------|------------|-----------|------------------|-----------|-----|----------|
| **Eco**⁴ | powersave | power | 5% | 25W³ | Auto | バッテリー駆動時間の最大化 |
| **Quiet** | powersave | power | 10% | 40W³ | Auto | 静音作業 |
| **Balanced** | powersave | balance_performance | 17% | 80W³ | Auto | 一般用途 |
| **Performance** | powersave¹ | performance | 50% | 100W³ | Max | ゲーミング |
| **Turbo** | performance² | 0（カーネルによる強制） | 100% | 110W³ | Max | 最大パフォーマンス |

いずれかのプロファイルを選択すると、そのファンモードも同時に適用されます。別途の操作は必要ありません。
PerformanceまたはTurboを選ぶと、（物理的なTurboキーと同様に）ファンはMaxに切り替わります。
Quiet、Balanced、Ecoではファンは Autoのままです。

⁴ バッテリー駆動時専用で、公式Windowsアプリと同様に、ACの選択肢としてEcoが提示されることは一切ありません。そのため、このカードは電源が接続されていないときのみModeページに表示されます。
この階層について確認済みのAcerのワット数/EPP値は存在しないため、その設定は他の4段階のような実測値ではなく、Quiet自体の数値を下回る保守的な外挿値です。

¹ Intel P-StateのHWP `powersave`ポリシーは、汎用の最小周波数ガバナーではなく、動的なスケーリングアルゴリズムです。
機種固有の名前付きEPPを書き込み可能な状態に保つため、Performanceは50%から最大値までの動的な階層になります。

² HWP `performance`ポリシー自体がEPPを0に強制し、利用可能なP-stateの範囲をその上限に制限します。Predator Senseは、数値のEPP書き込みを要求する代わりに、このカーネルの挙動に依存しています。バックエンドは、CPUモデルのアローリストなしに、すべてのcpufreqポリシーから検出されます。他のドライバーは既存の`performance` + 名前付き`performance`のマッピングを維持し、EPPを持たないシステムでは、そのオプション制御のみがスキップされます。

³ `nvidia-smi -pl`によるベストエフォート方式で、下記のGPUダッシュボードの電力制限スライダーと同様です — `nvidia-smi`が存在しない場合は静かにスキップされ、一部のノートパソコンではvBIOSがNVMLの電力制限制御をまったく公開していません（`nvidia-smi -q`が`Power Management Object: N/A`を報告し、要求内容にかかわらずすべての`-pl`値が拒否されます）。これはファームウェアレベルの制限であり、このアプリ、あるいはいかなるLinuxソフトウェアでも変更できるものではありません。これを引き上げるには、`nvflash`のようなWindows専用ツールで別のvBIOSを書き込む必要があり、GPUを文鎮化する現実的なリスクを伴う、まさに所有者自身の判断に委ねられる行為です。

**公式Windowsアプリとの既知の違い：** Quietモードでは、公式PredatorSenseはNVIDIAのWhisper Mode（`NvAPI_NvToppsJpacSetControl`）もオンにし、フレームレートを60FPSに制限してファンカーブをより静かに動作させます。この制御はNVIDIAのWindows専用ドライバーAPIの一部であり、Linuxには相当する機能がないため、同じハードウェア上でも、ここでのQuietは負荷がかかった状態ではWindowsのQuietほど静かにはなりません。これはプラットフォームの制約であり、このアプリのバグではありません。

### ファームウェアの電力プロファイル（推測ではなく実測）

上記の表にあるものはすべて、CPUとGPUの間で既存の電力予算を再分配するだけです。**パッケージ電力制限そのもの**はファームウェア自身のサーマルプロファイルによって設定されており、一部の機種ではファームウェアが最も低い階層で起動します — そのため、ガバナー、EPP、`min_perf`のいずれを変更しても、上限が1ワットたりとも引き上がることはありません。

`platform_profile`は、必ずしもこれらのモードに到達できるわけではありません。カーネルドライバーは、固定テーブル（`BALANCED=0, QUIET=1, PERFORMANCE=2, TURBO=3, ECO=4`）からこれらに名前を付けていますが、すべてのファームウェアでこれが成り立つわけではありません。Predator PHN16-73（Arrow Lake、BIOS V1.26）で、各生インデックスを書き込みパッケージ制限を読み戻して測定した結果は以下の通りです。

| ファームウェアインデックス | 持続（PL1） | バースト（PL2） | `platform_profile`での名称 |
|---:|---:|---:|---|
| 6 | 45 W | 50 W | *（なし — 到達不可）* |
| 0 | 55 W | 160 W | `balanced` |
| 1 | 70 W | 160 W | `quiet` |
| 4 | 95 W | 160 W | `low-power` |
| 5 | **115 W** | 160 W | *（なし — 到達不可）* |

最も強力なモードと最も弱いモードにはまったく名前がなく、名前が付いている3つも順序が誤ってラベル付けされています。修正済みのテーブルをハードコーディングしても、問題を次のファームウェアへ先送りするだけなので、Predator Senseは代わりに**実測します**。

1. カーネルモジュールは、生のインデックスと、ファームウェア自身がサポートするインデックスのビットマスクを、`/sys/devices/platform/acer-wmi/thermal_profile`および`thermal_profile_supported`として公開します。
2. **Mode → Calibrate profiles**は、サポートされている各インデックスを書き込み、`intel-rapl-mmio`から結果のパッケージ制限を読み取った上で、持続電力に基づいてランク付けします。数秒かかり、実行中はファンが聞こえるほど動きます。
3. これ以降、上記の4段階もファームウェアのプロファイルを連動して駆動するようになり、Quietが実際に最も弱いプロファイルへ、Turboが実際に最も強いプロファイルへ固定されます。

補足：

- **読み取り可能なRAPLを持たないマシン**（AMD機種や、より古いIntel機種）はランク付けできません。プロファイルは引き続き一覧表示され、手動で切り替え可能ですが、4段階のプロファイルは、順序を推測するのではなく、あえてファームウェアをそのままにしておきます — 上記のファームウェアでは、インデックスに基づく推測を行うとTurboが45Wのプロファイルになってしまいます。
- ファームウェアは電源サイクルのたびにプロファイルを**忘れる**ため、起動サービスが直前に選択されていたプロファイルを再適用します。
- ファームウェアがキーボードのライティングを電源モードに結びつけている機種では、切り替えのたびに（キャリブレーションの各ステップも含めて）キーボードが再描画されます。これはファームウェアによるものであり、このアプリの動作ではありません。気になる場合は、後でライティングページから色を再適用してください。
- 物理的な**モード切替キー**も、同じ実測済みの順序で循環します。詳しくは下記を参照してください。

### 物理モード切替キー

一部の機種には、電源モードを順に切り替える専用キーがあります。このキーは、組み込みコントローラー上の生のHID入力レポートとしてのみ報告され、input-subsystemのイベントを一切生成しません。そのため、PredatorSenseキー（WMIホットキー）は動作するのに、このキーはLinux上では反応がないように見えます。

デーモンは、このキーを検出するためにAcer EC HIDデバイスを監視します。デフォルト値はPHN16-73（`1025:174B`、レポート`04 85 ff`）で取得されたものです。他の機種では異なることが予想されるため、両方とも再ビルドなしで上書き可能です。

`~/.config/predator-sense/mode_key.json`：

```json
{ "product": "0000ABCD", "report": [4, 133, 255] }
```

（厳密なJSONです — このファイルに`//`コメントを入れるとパースできなくなり、デーモンはログにその旨を記録した上でデフォルト値にフォールバックします。）

キーが何も反応しない場合、デーモンは起動時に見つかったすべてのAcer HIDデバイスをログに記録します（設定画面で`debug_logging`を有効にしてください）。キーを押しながら`sudo hexdump -C /dev/hidrawN`で正しいデバイスを特定し、そのファイルをそこに設定してください — そして、その値を添えてissueを開いていただければ、お使いの機種のデフォルト値として取り込むことができます。

またファームウェアは、バッテリー残量が40%を下回るとモード切替を拒否します。デーモンは、キーが壊れているように見せる代わりに、この状況を報告します。

### 電源に応じた自動プロファイル

設定画面で有効にすると（新規インストールではデフォルトで有効）、これは単に電源の抜き差しに対する反応ではなく、継続的に適用されます。
- **AC接続時：** 常にPerformanceまたはTurbo。このいずれかがすでにアクティブな場合はそのまま維持されます — 自動切り替え機能が、両者間の手動選択と競合することはありません。
- **バッテリー駆動時：** 常にBalancedまたはQuietで、Performance/Turboになることはありません。バッテリー残量が15%を下回ると、設定されているターゲットにかかわらずQuietが強制されます。

### GPUダッシュボード

リアルタイムのNVIDIA GPUモニタリング：
- 温度、使用率、VRAM使用量、消費電力（円形ゲージ）
- 温度と使用率のライブ履歴グラフ（2分間のウィンドウ）
- コアクロック、メモリクロック、P-State、PCIeリンク情報、VBIOSバージョン

### AIアシスタント（ベータ版）

[Ollama](https://ollama.com)を利用した、オプトイン方式のローカルAIアシスタントです。すべてお使いのマシン上で完結して動作し、どこにも何も送信されません。

1. [公式Linux手順](https://ollama.com/download/linux)に従って、Ollamaを別途インストールしてください
2. サイドバーの**AI**に移動し、内蔵のモデルマネージャーからモデルをダウンロードしてください（`smollm2:1.7b`以上 — より小さいモデルはtool-callingを確実にはサポートしません）
3. **設定**でアシスタントを有効にし、**自動適用**（提案をすぐに適用）または**常に確認**（デフォルト — 提案された変更はすべて承認を待ちます）を選択してください

このアシスタントは、ライブのハードウェア状態（温度、ファン、サーマルプロファイル、バッテリー）を読み取り、固定された検証済みのアクション集合を通じて変更を提案または適用できます — ハードウェア/ECへの生のアクセスに直接触れることは一切なく、すべてのアクションは、AI機能が存在する以前からこのアプリがすでに使用していた関数と1対1で対応しています。モデルは分析を実行するためだけに読み込まれ、その後アンロードされます — メモリ上にアイドル状態で常駐し続けることはありません。すべてのAIの活動は、同じページ上の永続的で確認可能なアクションログに記録されます。

---

## インストーラーのオプション

Rust製インストーラーは、対話型のTUIを提供します。

```console
sudo ./predator-sense-installer              # 対話型メニュー
sudo ./predator-sense-installer --install    # 直接フルインストール
sudo ./predator-sense-installer --uninstall  # すべてを削除
sudo ./predator-sense-installer --reload-module # カーネルモジュールを再ビルド/再読み込み
sudo ./predator-sense-installer --status     # コンポーネントの状態を表示
```

---

## アンインストール

```console
sudo ./predator-sense-installer  # オプション2を選択
```

または手動で：
```console
pkill -f "/opt/predator-sense/predator-sense"
sudo rm -rf /opt/predator-sense
sudo rm -f /usr/share/applications/predator-sense.desktop
sudo rm -f /usr/share/icons/hicolor/128x128/apps/predator-sense.png
rm -f ~/.config/systemd/user/predator-sense-hotkey.service
rm -f ~/.config/autostart/predator-sense-hotkey.desktop
sudo rmmod facer  # 任意：カーネルモジュールをアンロード
```

---

## トラブルシューティング

<details>
<summary><b>キーボードRGBが変化しない / 特定のエフェクトで固まる</b></summary>

カーネルモジュールの状態が固まっている可能性があります。再読み込みしてください。
```console
sudo rmmod facer
sudo insmod /path/to/kernel/facer.ko
# またはインストーラーを使用: sudo ./predator-sense-installer → オプション4
```
</details>

<details>
<summary><b>モジュールが読み込まれない</b></summary>

```console
# WMIデバイスが存在するか確認
ls /sys/bus/wmi/devices/7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56/

# カーネルログを確認
sudo dmesg | grep -i facer

# ヘッダーがお使いのカーネルと一致しているか確認
sudo apt install linux-headers-$(uname -r)
```
</details>

<details>
<summary><b>PredatorSenseキーが動作しない</b></summary>

```console
# Rustホットキーサービスを確認
systemctl --user status predator-sense-hotkey.service
pgrep -af predator-sense-hotkey

# ユーザーが'input'グループに属しているか確認（追加後は完全なログアウト/ログイン、または再起動が必要です）
groups | grep input
sudo usermod -aG input $USER
```
</details>

<details>
<summary><b>NVIDIA GPUページにデータが表示されない</b></summary>

```console
# nvidia-smiが動作するか確認
nvidia-smi
# 動作しない場合は、NVIDIAのプロプライエタリドライバーをインストールしてください
```
</details>

<details>
<summary><b>お使いの機種に対応するquirkがない（プロファイル/ファン読み取り/PWMが欠けている）</b></summary>

お使いの正確な機種がまだ互換性リストに載っていない場合は、`predator_v4`系のオプション機能をすべて強制的に有効にし、実際にお使いのハードウェアで何が動作するかを確認してください。

```console
sudo modprobe facer enable_all=1
# 再起動後も持続させる場合:
echo "options facer enable_all=1" | sudo tee /etc/modprobe.d/facer-options.conf
```

これはWMIのみを使用するため（生のEC書き込みは行いません）、ある機能を実装していないハードウェアでは、不正な書き込みではなく安全なno-opになります。お使いの機種と、何が動作し何が動作しなかったかを添えて[issueを開いて](https://github.com/cleyton1986/predator-sense/issues)ください — こうして新しいquirkが追加されていきます。
</details>

---

## プロジェクト構成

```
predator-sense-gui/
├── kernel/                      # Linuxカーネルモジュール（DKMSで管理）
│   ├── facer.c                  # AcerハードウェアへのACPI/WMIインターフェース
│   ├── acer-wmi-battery.c       # バッテリー充電制限のサポート
│   ├── acpi_ec.c                # /dev/ec経由の生のECアクセス（MusiKid/acpi_ecより）
│   ├── Makefile
│   └── dkms.conf                # DKMS自動再ビルド設定
├── installer/                   # Rust製マルチコールインストーラーとサービス
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # インストール済みの実行ファイル名による型付きディスパッチ
│       ├── constants.rs         # 中央集権的なパス、プロトコル値、ハードウェア定数
│       ├── install.rs           # インストーラー + DKMS登録
│       ├── helper.rs            # 検証済みの特権ハードウェア操作
│       ├── hotkey.rs            # Linux入力イベントリスナー
│       ├── tray.rs              # StatusNotifierItemサービス
│       └── i18n.rs              # 型付きEN/PTメッセージ
├── protocol/                    # 共有される型付きGUI/ヘルパー契約
│   ├── Cargo.toml
│   └── src/lib.rs               # アクション、パス、上限値、バイナリ名
├── src/                         # Rust製GTK4アプリケーション
│   ├── main.rs
│   ├── app_state.rs             # グローバルなウィンドウ表示状態フラグ（タイマーをゲート）
│   ├── i18n.rs                  # EN/PT国際化
│   ├── config.rs                # ユーザー設定（JSON）
│   ├── tray.rs                  # Rust製トレイサービスのライフサイクル
│   ├── hardware/
│   │   ├── helper.rs            # 型付き特権ヘルパークライアント
│   │   ├── rgb.rs               # /dev/acer-gkbbl-*経由のRGB
│   │   ├── hwmon.rs             # /sys/class/hwmonインデックス（OnceLockでキャッシュ）
│   │   ├── sensors.rs           # 温度、ファン、RAM、ネットワーク
│   │   ├── gpu.rs               # TTLキャッシュ付きnvidia-smiパーサー
│   │   ├── procs.rs             # /procサンプラー（コアごとのCPU、メモリ、プロセス一覧）
│   │   ├── storage.rs           # df経由のディスク使用量
│   │   ├── sysinfo.rs           # DMI + CPU + GPU + OSの仕様
│   │   ├── fan.rs               # ファンモード + CoolBoost
│   │   ├── extras.rs            # バッテリー制限、LCDオーバードライブ、USB充電、起動アニメーション
│   │   ├── profile.rs           # CPUガバナー + EPP + GPU電力
│   │   ├── ai_assistant.rs      # Ollamaのtool-calling：既存のhardware::セッターにマッピングされた固定アローリスト
│   │   ├── ai_snapshot.rs       # AIに供給される一時的なハードウェア状態スナップショット。読み取りごとにクリアされる
│   │   ├── ai_actionlog.rs      # AIが提案/適用したすべての内容の永続的で確認可能なログ
│   │   └── setup.rs             # カーネルモジュール管理
│   └── ui/                      # GTK4ページ（Cairoカスタムウィジェット）
│       ├── window.rs            # メインウィンドウ、サイドバー、ネオンバー、トレイへの非表示化
│       ├── dashboard_page.rs    # ヒーロー表示 + システム仕様
│       ├── temperatures_page.rs # すべての温度ゲージ
│       ├── usage_page.rs        # 上位プロセス付きのCPU/GPU/メモリ/ストレージ
│       ├── network_page.rs      # ピーク値追跡付きのダウンロード/アップロード
│       ├── rgb_page.rs          # 視覚的なゾーン表示付きのキーボードRGB
│       ├── fan_control_page.rs  # アニメーション付きファン + CoolBoost
│       ├── fan_page.rs          # パフォーマンスプロファイル
│       ├── battery_page.rs      # バッテリー統計 + 充電制限
│       ├── gpu_page.rs          # NVIDIA GPUダッシュボード
│       ├── monitor_page.rs      # 詳細なCPU/GPU履歴グラフ
│       ├── ai_page.rs           # AIアシスタント：チャット、モデルマネージャー、リソースモニター、アクションログ
│       ├── setup_page.rs        # カーネルモジュールセットアップウィザード
│       └── gauge_widget.rs      # 破線の円形ゲージウィジェット
└── resources/
    ├── style.css                # ゲーミングダークテーマ
    └── predator-icon.svg        # システムトレイアイコン
```

---

## クレジットと謝辞

- **カーネルモジュール`facer`** は、[JafarAkhondali](https://github.com/JafarAkhondali)氏と[すべての貢献者](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module/graphs/contributors)による[acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module)プロジェクトをベースにしています
- **カーネルモジュール`acpi_ec`** は[Sayafdine Said (MusiKid)](https://github.com/MusiKid/acpi_ec)氏によるもので、生のEC読み書き用に`/dev/ec`を公開します。ヘルパーがファンモード、CoolBoost、LCDオーバードライブ、USB充電、起動アニメーションを設定する際に使用します。
- **GUIアプリケーション** は[Rust](https://www.rust-lang.org/) + [GTK4](https://gtk.org/) + [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/)で構築されています
- **インストーラーとバックグラウンドサービス** は[Rust](https://www.rust-lang.org/)で構築されており、トレイ統合には[ksni](https://crates.io/crates/ksni)を使用しています
- **DashboardとTemperaturesのアイコン**（`predator-sense-gui/resources/icons/`）は[Flaticon](https://www.flaticon.com)より、Hilmy Abiyyu A.氏、magnific氏、mehwish氏が制作しました

### このプロジェクトをフォーク・再利用する

このプロジェクトはGPL-3.0の下でライセンスされているため、同じライセンスの下で自由にフォーク、変更、再配布していただけます。もしそうする場合は — 特に派生アプリを構築する場合や、GUI/カーネルモジュールの重要な部分を再利用する場合は — **元の作者への目に見えるクレジット表記を残してください**（あなたのREADME、アプリ情報画面、クレジットセクションなどに、[Cleyton Alves](https://github.com/cleyton1986)氏 / このリポジトリへの言及があれば十分です）。個人の無償のサイドプロジェクトにとって、これはささやかなお願いですが、大きな支えになります。

## プロジェクトを支援する

このプロジェクトが役に立ち、開発を支援したいと思っていただけた方は、コーヒーをおごっていただけると嬉しいです。

<p align="center">
  <a href="https://www.paypal.com/cgi-bin/webscr?cmd=_donations&business=cleyton1986%40gmail.com&currency_code=BRL&item_name=Predator+Sense+for+Linux">
    <img src="https://img.shields.io/badge/PayPal-Donate-00457C?logo=paypal&logoColor=white&style=for-the-badge" alt="Donate via PayPal">
  </a>
</p>

<p align="center">
  <b>PIX（ブラジル）：</b> <code>cleyton1986@gmail.com</code>
</p>

すべての支援は任意であり、大変ありがたく思います！プロジェクトを存続させ、新機能開発の励みになります。

---

## ライセンス

このプロジェクトは**GNU General Public License v3.0**の下でライセンスされています — 詳細は[LICENSE](LICENSE)ファイルを参照してください。

これはフリーソフトウェアです。Free Software Foundationが公開するGNU GPLの条項の下で、再配布および/または変更することができます。

**例外 — 製品画像：** 上記のGPLv3ライセンスは、このプロジェクトのソースコードのみを対象としています。`predator-sense-gui/resources/models/`にあるAcer Predator/Nitroのノートパソコン写真はサードパーティの製品画像であり（上記の[免責事項](#免責事項)を参照）、GPLv3の許諾範囲には**含まれません**。これらの画像に関するすべての権利は、Acer Inc.および/または元の撮影者に帰属します。

**このソフトウェアは、いかなる保証もなく「現状のまま」提供されます。** 作者は、このソフトウェアの使用に起因するいかなる損害についても責任を負いません。このソフトウェアをインストールおよび使用することにより、あなたはそれを自己責任で行うことに同意したものとみなされます。
