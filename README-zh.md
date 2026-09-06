# Predator Sense for Linux（简体中文）

<p align="center">
  <a href="README.md">🇺🇸 Read in English</a> · <a href="README-ptbr.md">🇧🇷 Leia em Português</a> · <a href="README-es.md">🇪🇸 Leer en Español</a> · <a href="README-ja.md">🇯🇵 日本語で読む</a> · <a href="README-ru.md">🇷🇺 Читать на русском</a> · <a href="README-de.md">🇩🇪 Auf Deutsch lesen</a> · <a href="README-it.md">🇮🇹 Leggi in Italiano</a> · <a href="README-tr.md">🇹🇷 Türkçe Oku</a>
</p>

<p align="center">
  <img src="predator-sense-gui/resources/logo.jpeg" width="120" alt="Predator Sense Logo">
</p>

<p align="center">
  <b>面向 Acer 游戏本硬件控制的非官方 Linux 内核模块与图形界面</b><br>
  <i>RGB 键盘背光 &bull; Turbo 模式 &bull; 温度监控 &bull; 性能模式</i>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Language-Rust-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/GTK-4-blue?logo=gtk" alt="GTK4">
  <img src="https://img.shields.io/badge/Userspace-100%25_Rust-orange?logo=rust" alt="100% Rust userspace">
  <img src="https://img.shields.io/badge/License-GPL--3.0-green" alt="License">
  <img src="https://img.shields.io/badge/Platform-Linux-yellow?logo=linux" alt="Linux">
</p>

<p align="center">
  由 <a href="https://github.com/cleyton1986">Cleyton Alves</a> 创建和维护
</p>

---

## 免责声明

> **警告**
> **使用风险自负！** 这是一个**非官方**项目，Acer 并未参与其开发。该内核模块是通过对官方 PredatorSense Windows 应用进行逆向工程开发而成的。这个驱动会与底层的 WMI/ACPI 方法交互，这些方法并未在所有笔记本系列上测试过。作者不对你的硬件可能受到的任何损坏负责。

> **注意**
> 文中提到的所有商标、产品名称和标志（Acer、Predator、PredatorSense、Helios、Nitro、AeroBlade、CoolBoost）均属于其各自所有者（Acer Inc.）所有。本项目与 Acer Inc. 没有任何形式的关联、认可或赞助关系。

> **产品图片**
> `predator-sense-gui/resources/models/` 目录下的笔记本照片展示的是官方 Acer Predator/Nitro 产品，其唯一用途是让应用能够以图片形式识别出用户自己机器上检测到的型号（与系统 DMI/BIOS 报告的 `product_name` 进行匹配）。这些图片**不受本项目 GPLv3 许可覆盖**：底层产品摄影作品的版权归 Acer Inc. 和/或其原始创作者所有。收录这些图片完全出于善意，属于非商业性质、纯粹信息性的用途（指称性/产品识别用途），本项目对这些图片不主张任何所有权。如果你是版权所有者并希望移除某张图片，请提交一个 issue，图片会被尽快下架。

这个应用最初是为了**个人使用**而创建的，目的是在 Linux 上充分发挥 Acer 游戏本的性能，因为 Acer 并没有为 PredatorSense 提供官方的 Linux 支持。现在它被免费分享出来，供任何有同样需求的人使用。

如果这个应用/项目帮到了你，或者你以任何方式喜欢它，可以考虑点个 star，这会帮上很大的忙 ⭐

---

## 截图

<p align="center"><b>Dashboard</b>：笔记本照片和一目了然的完整系统规格，CPU、GPU、RAM、存储、网络和操作系统。</p>
<p align="center"><img src="assets/psense-1.png" width="800" alt="Dashboard"></p>

<p align="center"><b>温度</b>：CPU、GPU、系统、NVMe 硬盘、WiFi 和 RAM 的实时仪表盘，全部集中在一个界面中。</p>
<p align="center"><img src="assets/psense-2.png" width="800" alt="温度"></p>

<p align="center"><b>使用情况</b>：CPU、GPU、内存和存储，包含占用最高的进程、动画进度条和点击展开的详细信息（温度仪表盘上带有 CSS 风格的火焰动画）。</p>
<p align="center"><img src="assets/psense-3.png" width="800" alt="使用情况"></p>

<p align="center"><b>网络</b>：实时上传/下载图表，带有峰值追踪和自动网络接口检测（Wi-Fi 或以太网）。</p>
<p align="center"><img src="assets/psense-4.png" width="800" alt="网络"></p>

<p align="center"><b>灯光</b>：分区静态颜色（4 个分区）以及动态 RGB 键盘效果（Breathing、Neon、Wave、Shifting、Zoom）。</p>
<p align="center"><img src="assets/psense-5.png" width="800" alt="灯光"></p>

<p align="center"><b>模式</b>：性能模式，静音、均衡、性能和 Turbo，另外还有一个仅限电池模式下使用的 Eco 档位（CPU governor + Intel EPP + GPU 功耗限制）。</p>
<p align="center"><img src="assets/psense-6.png" width="800" alt="模式"></p>

<p align="center"><b>GameSync</b>：为某个游戏注册一个专属模式，游戏运行时应用会自动切换到该模式，游戏退出后再恢复之前生效的模式。</p>
<p align="center"><img src="assets/psense-15.png" width="800" alt="GameSync"></p>

<p align="center"><b>风扇控制</b>：实时转速显示，带有动画旋转的风扇、CoolBoost 开关以及 Auto/Max 模式。</p>
<p align="center"><img src="assets/psense-7.png" width="800" alt="风扇控制"></p>

<p align="center"><b>电池</b>：充电百分比、电压、电流、功率、循环次数、健康状况、制造商信息，以及用于延长寿命的 80% 充电限制。</p>
<p align="center"><img src="assets/psense-8.png" width="800" alt="电池"></p>

<p align="center"><b>GPU</b>：NVIDIA 面板，包含实时图表、频率、使用率、显存、功耗和 PCIe 信息。</p>
<p align="center"><img src="assets/psense-9.png" width="800" alt="GPU"></p>

<p align="center"><b>图表</b>：详细的 CPU 和 GPU 历史图表，带有最小值/最大值追踪。</p>
<p align="center"><img src="assets/psense-10.png" width="800" alt="图表"></p>

<p align="center"><b>AI 助手 (beta)</b>：由 Ollama 驱动的本地 AI 助手，聊天、模型管理器（列出已安装模型、下载新模型、选择运行哪一个）、思考过程中的实时 VRAM/GPU 资源占用，以及持久化的操作日志。</p>
<p align="center"><img src="assets/psense-11.png" width="800" alt="AI 助手"></p>

<p align="center"><b>驱动与手册</b>：显示序列号（带复制按钮）和指向 Acer 官方驱动与手册页面的直接链接，另外还有一张图示，标出笔记本上序列号贴纸的位置。</p>
<p align="center"><img src="assets/psense-16.png" width="800" alt="驱动与手册"></p>

<p align="center"><b>设置</b>：最小化到系统托盘、开机自启、启动时自动应用模式、语言偏好设置，以及按机型显示的受支持功能列表。</p>
<p align="center"><img src="assets/psense-12.png" width="800" alt="设置"></p>

<p align="center"><b>顶盖 Logo 灯光</b>：在支持彩色顶盖 Logo 的机型上，为显示器背面的 Logo 提供独立的 RGB 控制（Static/Breathing/Neon）。运行时检测：只有当硬件对能力探测有响应时，该控制项才会出现，因此在不支持的机型上会安全地保持隐藏。</p>
<p align="center"><img src="assets/psense-13.png" width="800" alt="顶盖 Logo 灯光"></p>
<p align="center"><img src="assets/psense-14.jpg" width="800" alt="在 Predator PHN16-73 上亮起绿色的顶盖 Logo"></p>
<p align="center"><sub>该功能由 <a href="https://github.com/jlucaso1">@jlucaso1</a> 贡献，在他自己的 Predator PHN16-73 上测试。这台笔记本的顶盖 Logo 不支持彩色，因此该功能是通过他的硬件验证的。</sub></p>

---

## 关于

非官方 Linux 内核模块，用于 Acer 游戏本的 RGB 键盘背光和 Turbo 模式（Acer Predator、Acer Helios、Acer Nitro）。

灵感来源并基于 [JafarAkhondali](https://github.com/JafarAkhondali) 及其他贡献者的 [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) 项目。本项目在现有 Linux Acer-WMI 内核模块的基础上进行扩展，以支持 Acer 游戏相关功能，并新增了一个使用 Rust 和 GTK4 构建的完整图形界面桌面应用程序。

---

## 功能特性

| 功能 | 说明 |
|---------|-------------|
| **Dashboard** | 笔记本照片 + 完整系统规格（CPU、GPU、RAM、存储、网络、操作系统）|
| **温度** | CPU、GPU、系统、NVMe、WiFi 和 RAM 的实时仪表盘 |
| **使用情况** | 4 个标签页视图：CPU / GPU / 内存 / 存储，包含占用最高的进程、点击展开的详细信息，以及温度仪表盘上的 CSS 风格火焰动画 |
| **网络** | 实时上传/下载图表，带有峰值追踪和自动网络接口检测 |
| **RGB 键盘控制** | 通过 WMI 实现分区静态颜色（4 个分区）和动态效果（Breathing、Neon、Wave、Shifting、Zoom）。在没有内核模块的硬件上，RGB 则通过 USB/I2C-HID 原生工作：ENEK5130 芯片（4 分区静态、Breathing/Neon）、2024+ Sunrex 芯片（单分区、完整效果列表）或 Chicony 芯片（7 色调色板、Helios 300），均为自动检测，参见[兼容性](#兼容性) |
| **RGB 顶盖 Logo** | 为显示器背面的徽标提供独立的开关、纯色、亮度、Breathing 和 Neon 控制，并带有实时矢量预览。仅在运行时完成 HID 能力检测后才会显示 |
| **性能模式** | 静音 / 均衡 / 性能 / Turbo 模式，另外还有一个仅限电池模式下使用的 Eco 档位（CPU governor + Intel EPP + GPU 功耗限制）|
| **风扇控制** | 实时转速显示，带有动画旋转的风扇、CoolBoost 开关、Auto/Max 模式，另外还有实验性的单风扇 PWM 控制和自动温度曲线（在支持的机型上）|
| **电池** | 充电统计信息、循环次数、健康状况、制造商信息，以及用于延长寿命的 80% 充电限制 |
| **GPU 面板** | NVIDIA 指标：温度、使用率、显存、频率、功耗、PCIe 信息，均带实时图表，另外还有一个**功耗限制（TGP）滑块** |
| **图表** | 详细的 CPU 和 GPU 历史图表，带有最小值/最大值追踪 |
| **AI 助手** 🧪 | 由 [Ollama](https://ollama.com) 驱动的本地、可选启用 AI 助手，读取实时硬件状态，并通过一组固定的、已验证的操作（散热模式、风扇模式、CoolBoost、RGB、GPU 功耗限制、电池）提出或应用更改建议。聊天、模型管理器（下载/选择）、实时资源/VRAM 监控，以及持久化的操作日志。自动应用或始终确认，由你选择。需要单独安装 Ollama，详见下方的[AI 助手](#ai-助手-beta) |
| **自动能力检测** | 检测每个机型支持哪些功能，并据此调整界面：不支持的功能会显示为“此机型不支持”，而不是报错。受支持的功能会在设置中列出 |
| **温度告警** | 当 CPU/GPU 超过 90°C 时发送桌面通知（在托盘中也能正常工作）|
| **电源自动切换** | 在接通电源/使用电池之间切换时自动更换模式：每种状态对应的目标模式可以在设置中配置（默认：接通电源时为性能，电池模式下为均衡）|
| **调试日志** | 设置中的可选开关：将守护进程和应用事件记录到 `~/.local/share/predator-sense/`（滚动日志，5MB×3），便于远程排查问题。默认关闭 |
| **系统托盘** | 使用 Predator 图标最小化到系统托盘：应用会在后台保持运行 |
| **PredatorSense 键** | 硬件按键映射：NumLock 旁边的按键可以打开应用 |
| **DKMS** | 内核升级时自动重新编译内核模块 |
| **国际化** | 根据系统区域设置自动切换英语 / 葡萄牙语 |
| **游戏风格界面** | 深色主题，带有脉动霓虹条、虚线圆形仪表盘、多边形面板边框。强调色会根据检测到的品牌自动确定：Predator/Helios/Triton 为青色，Nitro 为橙红色（与 NitroSense 一致），没有可手动切换的设置项 |

---

## 兼容性

**这能在我的笔记本上用吗？**

图例：✅ 已测试且可用 · 🟡 已实现，未测试（需要测试者）· 🧪 实验性（需要测试者）· ❌ 不可用 · `-` 不适用

| 产品名称 | Turbo（实现）| Turbo（测试）| RGB（实现）| RGB（测试）| 风扇转速读取 | 风扇模式 | 风扇 PWM % |
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

> 如果你的机型不在列表中，它仍然有可能可以正常工作：内核模块会自动检测兼容的 WMI 接口。如果在你的机器上能用（或不能用），请提交一个 issue 并注明你的机型，方便我们更新这张表。

### 风扇控制的三个级别

| 级别 | 功能 | 可用性 |
|---|---|---|
| **风扇转速读取** | 读取 CPU/GPU 风扇转速（`fan1_input`、`fan2_input`）| 大多数游戏本机型（自动检测）|
| **风扇模式** | 通过 `platform_profile` 实现静音 / 均衡 / 性能 / Turbo | `predator_v4` 机型 |
| **风扇 PWM %** 🧪 | 单风扇转速控制（`pwm1`/`pwm2` 0–100%），通过 WMI 从主线 `acer-wmi` 移植而来，**仅限内核 ≥ 6.14** | 支持 `ACER_CAP_PWM` 的部分机型（AN515-58、PHN16-72/73、PH16-72 等）|

> **🧪 PWM 风扇控制是实验性功能。** 它是从上游 Linux 内核的 `acer-wmi` 驱动移植而来，使用安全的 WMI 方法（不会直接写入 EC），但维护者**尚未在真实硬件上验证过**（维护者自己使用的是 PH315-54，该机型没有 PWM）。如果你的机型受支持，非常欢迎提供测试报告。**使用风险自负**，详见顶部的免责声明。

### 替代方案：linuwu_sense（Turbo 无法工作的未适配硬件）

`facer` 的 `enable_all=1` 回退模式可以识别任何支持 Acer WMI 的主板，但完整的 `predator_v4` 模式集（包含 `balanced-performance`/`performance` 在内的 5 个模式，以及可写的 `turbo_state`）只适用于其 DMI quirk 表中收录的主板。在未收录的主板上，`platform_profile_choices` 只能显示 `low-power quiet balanced`，即使固件支持更多模式，`turbo_state` 也依然是只读的。这一情况在一台 PHN16-73（Macan_ARX，BIOS V1.26）上被报告于 [#33](https://github.com/cleyton1986/predator-sense/issues/33)。

如果你遇到的正是这种情况，社区维护的 [Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense) 模块（以 `predator_v4=1` 加载）会通过本应用已经直接读取的那些通用 `platform_profile`/`intel_pstate`/`acer-wmi-battery` 接口，暴露出完整的模式集，不涉及任何 `facer` 专有的代码路径。从 `v0.2.71-preview` 起，本应用会检测 `linuwu_sense`，当实际提供这些接口的驱动就是它时，会跳过“安装 facer”的提示。RGB 和散热模式校准（两者都仅限 `facer`，见上文和下文）仍然需要 `facer` 本身，在 linuwu_sense 下依然不可用。

### 不使用内核模块的 RGB（仅限 I2C-HID 硬件）

部分机型（已确认：PHN16S-71、PHN16-73、AN16S-61）会将键盘的 RGB 控制器路由到一个独立的 I2C-HID 芯片（ENEK5130），而不是 `facer.ko` 的 WMI 接口。本应用会通过 `/dev/hidrawN` 直接与它通信，因此即使完全没有加载内核模块，这些功能也能正常工作：

| 功能 | 状态 |
|---|---|
| 分区静态颜色、亮度、关闭背光 | ✅ 已确认可用（PHN16S-71、AN16S-61）|
| 动态效果：Breathing、Neon | ✅ 已确认可用（PHN16S-71、AN16S-61）。原生实现，单次 HID 写入，硬件会自行循环播放图案。在 PHN16S-71 上，Breathing 会忽略所选颜色，改为循环彩虹色；在其他硬件上可能表现不同 |
| 动态效果：Wave、Shifting、Zoom | 仅提供屏幕预览（不会写入硬件）。这些效果的代码在不同硬件世代之间含义并不相同，所以尚未接入 |
| RGB 顶盖 Logo：关闭、纯色、亮度、Breathing、Neon | ✅ 已确认可用（PHN16-73）|

顶盖 Logo 的支持并不是通过机型名称的白名单来启用的。控制器必须在其 A1 目标报告中声明 `0x83` 目标，并返回与之匹配、非空的 A3 能力信息，界面才会显示出来；本应用会在每次写入之前重复执行这项检查。hotkey 守护进程只会恢复应用此前在登录和恢复后成功应用过的设置，如果没有已保存的设置或目标不存在，则会完全跳过 Logo。

一份关于 [AN16S-61 的独立报告](https://github.com/cleyton1986/predator-sense/issues/31)（另见报告作者自己的[独立协议工具](https://github.com/ArnarValur/Nitro16S-AI-RGB-Keyboard)）在 static/Breathing/Neon/Wave 之外，又映射出六种额外的原生线协议模式（一个硬件关闭模式、一个由 EC 自身触发的开机闪烁模式，以及另外四种内置动画），还有一个模式/turbo 键的 LED 目标。这些目前都还没有接入应用，需要先为硬件原生效果代码定义一个专门的位置才能接入，因此被记录为未来的改进项。

同一份报告还包含了直接从控制器提取的、已解码的 HID report descriptor，这解决了一个真实存在的 bug：应用之前是从错误的字节（`byte[3]`，一个按目标类别固定的常量）读取 A3 能力报告中的分区数量，而不是控制器自身描述符中真正声明该字段的字节（`byte[4]`）。这个问题已在 `v0.2.69-preview` 中修复，应用和 hotkey 守护进程都做了修复。这是一个协议层面的修正，而不是针对特定机型的改动：report descriptor 的字段布局来自芯片自身的固件（三个已确认机型使用的都是同一颗 `0CF2:5130` 芯片），并且不会改变任何已确认可用硬件上的线上字节，因为之前的值本来就是正确值的一个更宽泛的超集。

### 2024+ 硬件上的 RGB（Sunrex/Darfon USB HID）

更新的一代产品（PH16-72 以及其他共享相同 USB HID 芯片的 2024-2026 机型，参见 issue #26）把键盘和顶盖 Logo 的 RGB 同时从 WMI *和* 上面提到的 ENEK5130 芯片上移开，改用了完全不同的一对控制器：键盘用 Sunrex `05af:*`，Logo 用 Darfon `0d62:*`。本应用也能直接检测并驱动这些控制器，只要检测到它们存在，就会自动优先于 ENEK5130/WMI 路径：

| 功能 | 状态 |
|---|---|
| 键盘：关闭、静态、Breathing、Wave、Snake、Neon、Spot、Star、Rainbow、5× Slash、Zoom、Row Wave、Swiping | 🟡 已实现，等待真实硬件确认 |
| 顶盖 Logo：关闭、纯色、亮度、Breathing | 🟡 已实现，等待真实硬件确认 |

这颗芯片没有独立分区：与上面 4 分区的 ENEK5130 控制器不同，整个键盘一次只能使用一种颜色/效果。这个线协议是从官方 Windows 应用的两个反编译版本中逐字节逆向出来的（两者的每一段固定字节序列和校验和公式都完全一致），并非猜测得来，但目前还没有人在真实硬件上验证过，因此在收到真实报告之前，请视为未经测试。

第三颗芯片（Chicony，Helios 300/PH317-56）使用的是另一种 USB HID 协议，由社区通过逆向工程记录（[NT411/Acer-Predator-Fan-RGB-Controller-Linux](https://github.com/NT411/Acer-Predator-Fan-RGB-Controller-Linux)），本应用是根据这份规范重新实现的：固定的 7 色调色板（这是硬件/固件的限制，不是任意 RGB）覆盖 12 种效果。同样是 🟡，等待确认。

### 已经在使用 Linuwu-Sense 或 DAMX？

[Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense)（以及基于它构建的 [DAMX](https://github.com/PXDiv/Div-Acer-Manager-Max)）是一个完全独立、不相关的项目，同样用于在 Linux 上驱动 Acer Predator/Nitro 硬件。它不是本项目的依赖项，本项目也没有使用它的任何代码，但它的内核模块绑定的是和 `facer` 相同的 WMI GUID，而内核不允许两个驱动同时占用同一个设备。

如果安装程序检测到 `linuwu_sense` 已经加载或已通过 DKMS 安装，会自动**保持你现有配置不变**：不会把 `acer_wmi` 加入黑名单，也不会强制加载 `facer`，因此不会和一个已经在正常工作的 Linuwu-Sense/DAMX 安装发生冲突（或破坏它）。无论当前激活的是哪个平台驱动，键盘 RGB 仍然会通过本应用经由 HID 路径（见上文）正常工作；在这种情况下，风扇/散热控制则继续交给你原本已经在使用的那个工具管理。

---

## 安装

### 预编译安装程序（最快）

直接下载 release 中的安装程序并运行：

```console
curl --fail --location https://github.com/cleyton1986/predator-sense/releases/latest/download/predator-sense-installer --output predator-sense-installer
chmod +x predator-sense-installer
sudo ./predator-sense-installer --install
```

安装程序、特权 helper、hotkey 监听器和托盘服务都由同一个 Rust multicall 二进制文件提供。安装程序会下载并配置好一切，不需要 shell 脚本作为引导。

### 交互式安装程序（预编译二进制文件，无需 Rust 工具链）

从 [Releases](../../releases) 页面下载 `predator-sense-installer` 二进制文件。它是一个独立的 Rust 二进制文件，不是一个打包好的软件包：仍然需要联网来获取应用的源码（用于内核模块）以及对应的预编译 release 二进制文件，但完全不需要在你的机器上安装 Rust 或编译 GTK4 应用：

```console
chmod +x predator-sense-installer
sudo ./predator-sense-installer
```

选择**选项 1**（完整安装）。安装程序会自动：

1. 检测你的发行版（Debian/Ubuntu/Mint、Fedora、Arch）
2. 安装系统依赖（GTK4、libadwaita、构建工具、内核头文件）
3. 下载对应 release 的源码 + 预编译二进制文件
4. 编译并加载 `facer` 内核模块（这部分始终在本地编译，因为内核模块无法跨不同内核版本预编译分发）
5. 创建带图标的桌面菜单项
6. 映射 PredatorSense 硬件按键（登录时自动启动）
7. 配置系统托盘支持

预编译方式不需要目标机器上安装 Rust/cargo。安装程序还会被复制到 `/opt/predator-sense/`，作为一个独立的管理工具，用于状态检查、内核模块重新加载、升级和卸载（参见[安装程序选项](#安装程序选项)）。

安装完成后，可以通过以下方式打开应用：
- 按下 **PredatorSense 键**（NumLock 旁边）
- 在应用程序菜单中搜索 **“Predator Sense”**
- 在终端中运行 `/opt/predator-sense/predator-sense`

### 手动安装（从源码构建）

#### 前置依赖

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

**Rust**（如果尚未安装）：
```console
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### 构建与安装

```console
# 克隆仓库
git clone https://github.com/cleyton1986/predator-sense.git
cd predator-sense/predator-sense-gui

# 构建 GUI 以及 Rust 安装程序/服务
cargo build --release
cargo build --release --manifest-path installer/Cargo.toml

# 安装本地构建产物，并将已有的 C 语言内核源码注册到 DKMS
sudo installer/target/release/predator-sense-installer --install

# 运行
/opt/predator-sense/predator-sense
```

---

## 使用方法

### 键盘 RGB

1. 打开侧边栏中的**灯光**
2. 选择**静态**（分区颜色）或**动态**（效果）
3. **静态模式：** 为键盘的 4 个分区分别调整 R/G/B 滑块
4. **动态模式：** 选择一种效果（Breathing、Neon、Wave、Shifting、Zoom）并调整速度
5. 点击**应用**

> 在只有 I2C-HID、没有内核模块的硬件上（参见[兼容性](#兼容性)），Breathing 和 Neon 会真正播放动画；Wave/Shifting/Zoom 目前只提供屏幕预览，并会明确标注为预览，物理键盘暂时还不会随之改变。

### RGB 顶盖 Logo

1. 打开**灯光**，选择**顶盖 Logo**（只有检测到兼容的 HID 目标时，该选项才会出现）
2. 使用**灯光**开关来打开或关闭徽标
3. 选择**静态**、**Breathing** 或 **Neon**，然后一边查看实时预览，一边调整可用的颜色、亮度和速度控制项
4. 点击**应用到 Logo**

上一次成功应用的状态会在用户 hotkey 服务启动时以及挂起/休眠之后被恢复。动态效果的颜色由固件控制，因此对于这些模式，预览展示的是其实际行为，而不是提供一个颜色选择器。

> 在 Linux 启动用户服务之前显示的灯光动画由固件掌控。已保存的“关闭”状态会在登录后被恢复，但本应用无法抑制更早的 BIOS/开机动画。

### 性能模式

在启用 Intel P-State + HWP 的系统上，CPU 一侧的具体表现如下：

| 模式 | HWP 策略 | Intel EPP | 最低性能 | GPU 功耗 | 风扇 | 使用场景 |
|---------|------------|-----------|------------------|-----------|-----|----------|
| **Eco**⁴ | powersave | power | 5% | 25W³ | Auto | 最大化续航 |
| **静音** | powersave | power | 10% | 40W³ | Auto | 安静办公 |
| **均衡** | powersave | balance_performance | 17% | 80W³ | Auto | 日常使用 |
| **性能** | powersave¹ | performance | 50% | 100W³ | Max | 游戏 |
| **Turbo** | performance² | 0（内核强制）| 100% | 110W³ | Max | 极限性能 |

选择任意一种模式时，也会同时应用其对应的风扇模式，不需要额外操作。
选择性能或 Turbo 会把风扇推到 Max（与物理 Turbo 键效果相同）；
静音、均衡和 Eco 则会让风扇保持在 Auto。

⁴ 仅限电池模式，与官方 Windows 应用保持一致：接通电源时完全不会
提供 Eco 选项，因此这张卡片只会在拔掉电源时出现在“模式”页面上。
这一档位没有经过 Acer 官方确认的功耗/EPP 数值，因此它的设置是在
“静音”自身数值基础上做的保守外推，而不是像另外四档那样经过实测。

¹ Intel P-State 的 HWP `powersave` 策略是一种动态调频算法，而不是
通用的最低频率 governor。它会保持该型号对应的具名 EPP 可写，这使得
“性能”成为一个从 50% 到最大值动态变化的档位。

² HWP 的 `performance` 策略本身会强制将 EPP 设为 0，并将可用的
P-state 范围限制在其上限。Predator Sense 依赖的正是这种内核行为，
而不需要直接写入数字形式的 EPP。后端会从每一种 cpufreq 策略中检测
得出，不依赖 CPU 型号白名单。其他驱动依然保留现有的 `performance`
加具名 `performance` 的映射方式，没有 EPP 的系统只会跳过这一项可选
控制。

³ 通过 `nvidia-smi -pl` 尽力而为实现，和下面 GPU 面板中的功耗限制
滑块相同：如果没有安装 `nvidia-smi`，会静默跳过；而在部分笔记本上，
vBIOS 根本不会暴露 NVML 的功耗限制控制（`nvidia-smi -q` 会报告
`Power Management Object: N/A`，无论请求什么数值，每一个 `-pl` 值
都会被拒绝）。这是固件层面的限制，不是本应用，或者任何 Linux 软件
能够改变的；要提高这个上限，就意味着要用 `nvflash` 这类仅支持
Windows 的工具刷入不同的 vBIOS，这存在让 GPU 变砖的真实风险，完全
是机主自己的选择。

**与官方 Windows 应用的已知差异：** 在静音模式下，官方 PredatorSense
还会开启 NVIDIA 的 Whisper Mode（`NvAPI_NvToppsJpacSetControl`），
将帧率限制在 60 FPS，让风扇曲线运行得更安静。这项控制是 NVIDIA 仅限
Windows 的驱动 API 的一部分，在 Linux 上没有等价实现，因此在相同硬件
上，这里的静音模式在负载下不会像 Windows 上的静音模式那样安静，这是
平台层面的限制，不是本应用的 bug。

### 固件功耗档位（实测而非猜测）

上面表格中的所有内容，都只是在 CPU 和 GPU 之间重新分配一个既有的
功耗预算。**封装功耗上限本身**是由固件自己的散热模式设定的，在部分
机型上，固件启动时就处于最低的那一档，因此无论怎么调整 governor、
EPP 或 `min_perf`，都无法把这个上限提高哪怕一瓦。

`platform_profile` 并不总能到达这些档位。内核驱动是根据一张固定的表
（`BALANCED=0, QUIET=1, PERFORMANCE=2, TURBO=3, ECO=4`）来命名这些
档位的，但这张表并不适用于所有固件。在一台 Predator PHN16-73
（Arrow Lake，BIOS V1.26）上实测，逐一写入每个原始索引并读回封装
功耗上限：

| 固件索引 | 持续功耗（PL1）| 突发功耗（PL2）| 通过 `platform_profile` 得到的名称 |
|---:|---:|---:|---|
| 6 | 45 W | 50 W | *（无，无法到达）* |
| 0 | 55 W | 160 W | `balanced` |
| 1 | 70 W | 160 W | `quiet` |
| 4 | 95 W | 160 W | `low-power` |
| 5 | **115 W** | 160 W | *（无，无法到达）* |

功耗最强和最弱的两个档位完全没有名字，而剩下有名字的三个又被贴错了
顺序标签。把一张“纠正过”的表硬编码进去，只会把问题转移到下一款
固件身上，所以 Predator Sense **选择实测**：

1. 内核模块会将原始索引，以及固件自身支持的索引位掩码，分别以
   `/sys/devices/platform/acer-wmi/thermal_profile` 和
   `thermal_profile_supported` 的形式暴露出来。
2. **模式 → 校准档位** 会依次写入每一个受支持的索引，并从
   `intel-rapl-mmio` 中读取由此得到的封装功耗上限，然后按持续功耗
   排序。这个过程只需要几秒钟，运行时能听到风扇明显转动。
3. 从那之后，上面这四个档位也会同步驱动固件模式，并以此为基准，让
   “静音”对应到实际最弱的档位，“Turbo”对应到实际最强的档位。

说明：

- **无法读取 RAPL 的机器**（AMD 机型、较旧的 Intel 机型）无法进行
  排序。这些档位仍然会被列出并且可以手动切换，但这四个档位会刻意不
  去干涉固件，而不是去猜一个顺序：以上面的固件为例，按索引猜测的话，
  会把 Turbo 放到 45 W 那一档上。
- 固件在每次断电重启后都会**遗忘**已选的档位，因此开机服务会重新
  应用你上一次选择的档位。
- 在固件把键盘灯光和功耗模式绑定在一起的机型上，每一次切换（包括
  校准过程中的每一步）都会重新绘制键盘灯光。这是固件的行为，不是
  本应用做的；如果这让你感到困扰，之后可以在“灯光”页面重新应用你
  的颜色设置。
- 物理**模式切换键**会按照相同的实测顺序循环切换；详见下文。

### 物理模式切换键

部分机型有一个专门用来循环切换功耗模式的按键。它只会在嵌入式控制器
上以原始 HID input report 的形式上报，完全不会产生 input 子系统的
事件，这就是为什么它在 Linux 上看起来毫无反应，而 PredatorSense 键
（一个 WMI hotkey）却能正常工作。

守护进程会监听 Acer EC 的 HID 设备来捕获这个按键。默认值是在一台
PHN16-73 上抓取的（`1025:174B`，report `04 85 ff`）；其他机型预计
会有所不同，因此这两个值都可以在不重新编译的情况下覆盖：

`~/.config/predator-sense/mode_key.json`：

```json
{ "product": "0000ABCD", "report": [4, 133, 255] }
```

（严格 JSON 格式：文件中如果出现 `//` 注释会导致解析失败，此时守护
进程会回退到默认值，并在日志中留下一条记录。）

如果这个按键没有任何反应，守护进程会在启动时把它找到的每一个 Acer
HID 设备都记录到日志中（在设置中启用 `debug_logging`）。按住这个
按键的同时运行 `sudo hexdump -C /dev/hidrawN` 找到正确的设备，然后
把配置文件指向它；也请把这些值提交为一个 issue，这样它们就可以成为
你的机型对应的默认值。

固件在电池电量低于 40% 时也会拒绝切换模式；守护进程会把这一情况报告
出来，而不是让这个按键看起来像是坏了。

### 按电源自动切换模式

在设置中启用后（新安装默认开启），这项功能不只是对插拔电源的一次性
反应，而是会持续生效：
- **接通电源时：** 始终为性能或 Turbo。如果这两者之一已经处于激活
  状态，就会保持不变，自动切换逻辑永远不会和你在这两者之间做出的
  手动选择相冲突。
- **使用电池时：** 始终为均衡或静音，绝不会是性能/Turbo。电量低于
  15% 时，无论配置的目标模式是什么，都会强制切换到静音。

### GPU 面板

实时 NVIDIA GPU 监控：
- 温度、使用率、显存占用、功耗（圆形仪表盘）
- 温度和使用率的实时历史图表（2 分钟窗口）
- 核心频率、显存频率、P-State、PCIe 链路信息、VBIOS 版本

### AI 助手 (beta)

一个可选启用的本地 AI 助手，由完全运行在你自己机器上的 [Ollama](https://ollama.com) 驱动，任何数据都不会被发送到任何地方。

1. 按照[官方 Linux 安装说明](https://ollama.com/download/linux)单独安装 Ollama
2. 打开侧边栏中的 **AI**，从内置的模型管理器中下载一个模型（`smollm2:1.7b` 或更大，更小的模型无法可靠地支持 tool-calling）
3. 在**设置**中启用该助手，并选择**自动应用**（立即应用建议）或**始终确认**（默认，每一条建议的更改都会等待你的确认）

该助手会读取实时硬件状态（温度、风扇、散热模式、电池），并可以通过一组固定的、已验证的操作提出或应用更改建议：它从不直接触碰底层硬件/EC，每一个操作都与本应用在 AI 功能出现之前就已经在使用的某个函数一一对应。模型只会在执行分析时被加载，之后就会被卸载，不会一直闲置在内存中。所有 AI 活动都会记录在同一页面上一份持久化、可查看的操作日志中。

---

## 安装程序选项

这个 Rust 安装程序提供一个交互式 TUI：

```console
sudo ./predator-sense-installer              # 交互式菜单
sudo ./predator-sense-installer --install    # 直接完整安装
sudo ./predator-sense-installer --uninstall  # 移除所有内容
sudo ./predator-sense-installer --reload-module # 重新编译/重新加载内核模块
sudo ./predator-sense-installer --status     # 显示各组件状态
```

---

## 卸载

```console
sudo ./predator-sense-installer  # 选择选项 2
```

或者手动执行：
```console
pkill -f "/opt/predator-sense/predator-sense"
sudo rm -rf /opt/predator-sense
sudo rm -f /usr/share/applications/predator-sense.desktop
sudo rm -f /usr/share/icons/hicolor/128x128/apps/predator-sense.png
rm -f ~/.config/systemd/user/predator-sense-hotkey.service
rm -f ~/.config/autostart/predator-sense-hotkey.desktop
sudo rmmod facer  # 可选：卸载内核模块
```

---

## 故障排除

<details>
<summary><b>键盘 RGB 不变化 / 卡在某个效果上</b></summary>

内核模块的状态可能卡住了。重新加载它：
```console
sudo rmmod facer
sudo insmod /path/to/kernel/facer.ko
# 或者使用安装程序：sudo ./predator-sense-installer → 选项 4
```
</details>

<details>
<summary><b>模块无法加载</b></summary>

```console
# 检查 WMI 设备是否存在
ls /sys/bus/wmi/devices/7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56/

# 检查内核日志
sudo dmesg | grep -i facer

# 确保头文件版本与你的内核匹配
sudo apt install linux-headers-$(uname -r)
```
</details>

<details>
<summary><b>PredatorSense 键不起作用</b></summary>

```console
# 检查 Rust hotkey 服务
systemctl --user status predator-sense-hotkey.service
pgrep -af predator-sense-hotkey

# 确保用户在 'input' 用户组中（添加后需要完整注销/登录或重启才能生效）
groups | grep input
sudo usermod -aG input $USER
```
</details>

<details>
<summary><b>NVIDIA GPU 页面没有数据</b></summary>

```console
# 确认 nvidia-smi 能正常工作
nvidia-smi
# 如果不行，安装 NVIDIA 专有驱动
```
</details>

<details>
<summary><b>我的机型没有匹配的 quirk（缺少档位/风扇读取/PWM）</b></summary>

如果你的具体机型还不在兼容性列表中，可以尝试强制开启所有可选的 `predator_v4` 系列功能，看看你的硬件上实际能用哪些：

```console
sudo modprobe facer enable_all=1
# 重启后依然生效：
echo "options facer enable_all=1" | sudo tee /etc/modprobe.d/facer-options.conf
```

这只涉及 WMI（不会直接写入 EC），因此在没有实现某项功能的硬件上，这是一次安全的空操作，而不是一次有害的写入。请[提交一个 issue](https://github.com/cleyton1986/predator-sense/issues)，说明你的机型以及哪些功能可用/不可用，新的 quirk 就是这样被添加进来的。
</details>

---

## 项目结构

```
predator-sense-gui/
├── kernel/                      # Linux 内核模块（由 DKMS 管理）
│   ├── facer.c                  # 面向 Acer 硬件的 ACPI/WMI 接口
│   ├── acer-wmi-battery.c       # 电池充电限制支持
│   ├── acpi_ec.c                # 通过 /dev/ec 进行原始 EC 访问（来自 MusiKid/acpi_ec）
│   ├── Makefile
│   └── dkms.conf                # DKMS 自动重新编译配置
├── installer/                   # Rust multicall 安装程序与服务
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # 根据已安装可执行文件名进行的类型化分发
│       ├── constants.rs         # 集中管理的路径、协议值和硬件常量
│       ├── install.rs           # 安装程序 + DKMS 注册
│       ├── helper.rs            # 经过校验的特权硬件操作
│       ├── hotkey.rs            # Linux input-event 监听器
│       ├── tray.rs              # StatusNotifierItem 服务
│       └── i18n.rs              # 类型化的 EN/PT 消息
├── protocol/                    # 共享的类型化 GUI/helper 契约
│   ├── Cargo.toml
│   └── src/lib.rs               # 操作、路径、限制和二进制文件名
├── src/                         # Rust GTK4 应用程序
│   ├── main.rs
│   ├── app_state.rs             # 全局窗口可见性标志（用于控制定时器）
│   ├── i18n.rs                  # EN/PT 国际化
│   ├── config.rs                # 用户偏好设置（JSON）
│   ├── tray.rs                  # Rust 托盘服务生命周期
│   ├── hardware/
│   │   ├── helper.rs            # 类型化的特权 helper 客户端
│   │   ├── rgb.rs               # 通过 /dev/acer-gkbbl-* 实现的 RGB
│   │   ├── hwmon.rs             # /sys/class/hwmon 索引（使用 OnceLock 缓存）
│   │   ├── sensors.rs           # 温度、风扇、RAM、网络
│   │   ├── gpu.rs               # 带 TTL 缓存的 nvidia-smi 解析器
│   │   ├── procs.rs             # /proc 采样器（每核 CPU、内存、进程列表）
│   │   ├── storage.rs           # 通过 df 获取磁盘使用情况
│   │   ├── sysinfo.rs           # DMI + CPU + GPU + 操作系统规格
│   │   ├── fan.rs               # 风扇模式 + CoolBoost
│   │   ├── extras.rs            # 电池限制、LCD 超频、USB 充电、开机动画
│   │   ├── profile.rs           # CPU governor + EPP + GPU 功耗
│   │   ├── ai_assistant.rs      # Ollama tool-calling：固定的白名单，映射到已有的 hardware:: setter
│   │   ├── ai_snapshot.rs       # 提供给 AI 的临时硬件状态快照，每次读取后清空
│   │   ├── ai_actionlog.rs      # AI 所有建议/应用操作的持久化、可查看日志
│   │   └── setup.rs             # 内核模块管理
│   └── ui/                      # GTK4 页面（Cairo 自定义控件）
│       ├── window.rs            # 主窗口、侧边栏、霓虹条、隐藏到托盘
│       ├── dashboard_page.rs    # 主视觉区 + 系统规格
│       ├── temperatures_page.rs # 全部温度仪表盘
│       ├── usage_page.rs        # 带占用最高进程的 CPU/GPU/内存/存储
│       ├── network_page.rs      # 带峰值追踪的下载/上传
│       ├── rgb_page.rs          # 带可视化分区的键盘 RGB
│       ├── fan_control_page.rs  # 动画风扇 + CoolBoost
│       ├── fan_page.rs          # 性能模式
│       ├── battery_page.rs      # 电池统计信息 + 充电限制
│       ├── gpu_page.rs          # NVIDIA GPU 面板
│       ├── monitor_page.rs      # 详细的 CPU/GPU 历史图表
│       ├── ai_page.rs           # AI 助手：聊天、模型管理器、资源监控、操作日志
│       ├── setup_page.rs        # 内核模块安装向导
│       └── gauge_widget.rs      # 虚线圆形仪表盘控件
└── resources/
    ├── style.css                # 游戏风格深色主题
    └── predator-icon.svg        # 系统托盘图标
```

---

## 致谢

- **内核模块 `facer`** 基于 [JafarAkhondali](https://github.com/JafarAkhondali) 及[所有贡献者](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module/graphs/contributors)的 [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) 项目
- **内核模块 `acpi_ec`** 由 [Sayafdine Said (MusiKid)](https://github.com/MusiKid/acpi_ec) 提供：暴露 `/dev/ec` 用于原始 EC 读写。helper 用它来设置风扇模式、CoolBoost、LCD 超频、USB 充电和开机动画。
- **GUI 应用程序** 使用 [Rust](https://www.rust-lang.org/) + [GTK4](https://gtk.org/) + [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/) 构建
- **安装程序与后台服务** 使用 [Rust](https://www.rust-lang.org/) 构建；托盘集成使用 [ksni](https://crates.io/crates/ksni)
- **Dashboard 和温度页面图标**（`predator-sense-gui/resources/icons/`）来自 [Flaticon](https://www.flaticon.com)，由 Hilmy Abiyyu A.、magnific 和 mehwish 创作

### Fork 或复用本项目

本项目基于 GPL-3.0 许可，因此你可以自由地 fork、修改并在相同许可下重新分发它。如果你这样做，尤其是构建了衍生应用，或者复用了 GUI/内核模块中相当一部分内容，**请为原作者保留一个可见的署名**（只需要在你的 README、关于页面或致谢部分提到 [Cleyton Alves](https://github.com/cleyton1986) / 本仓库即可）。对于一个独立、无偿的业余项目来说，这是一个很小的请求，但意义重大。

## 支持本项目

如果这个项目对你有帮助，并且你想支持它的开发，可以考虑请我喝杯咖啡：

<p align="center">
  <a href="https://www.paypal.com/cgi-bin/webscr?cmd=_donations&business=cleyton1986%40gmail.com&currency_code=BRL&item_name=Predator+Sense+for+Linux">
    <img src="https://img.shields.io/badge/PayPal-Donate-00457C?logo=paypal&logoColor=white&style=for-the-badge" alt="Donate via PayPal">
  </a>
</p>

<p align="center">
  <b>PIX (巴西):</b> <code>cleyton1986@gmail.com</code>
</p>

任何形式的支持都完全出于自愿，并且都非常感谢！这有助于让项目持续维护下去，也会激励我开发新功能。

---

## 许可证

本项目基于 **GNU General Public License v3.0** 许可，详情参见 [LICENSE](LICENSE) 文件。

这是自由软件：你可以依据自由软件基金会发布的 GNU GPL 条款对其进行再分发和/或修改。

**例外情况：产品图片。** 上述 GPLv3 许可仅覆盖本项目的源代码。`predator-sense-gui/resources/models/` 目录下的 Acer Predator/Nitro 笔记本照片属于第三方产品图片（参见上文的[免责声明](#免责声明)），**不**受 GPLv3 授权覆盖；这些图片的所有权利归 Acer Inc. 和/或原始摄影师所有。

**本软件按“原样”提供，不附带任何形式的保证。** 作者不对使用本软件可能造成的任何损害负责。安装并使用本软件，即表示你确认自行承担全部风险。
</content>
