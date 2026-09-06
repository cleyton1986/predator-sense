# Predator Sense para Linux

<p align="center">
  <a href="README.md">🇺🇸 Read in English</a> · <a href="README-ptbr.md">🇧🇷 Leia em Português</a> · <a href="README-zh.md">🇨🇳 阅读中文版</a> · <a href="README-ja.md">🇯🇵 日本語で読む</a> · <a href="README-ru.md">🇷🇺 Читать на русском</a> · <a href="README-de.md">🇩🇪 Auf Deutsch lesen</a> · <a href="README-it.md">🇮🇹 Leggi in Italiano</a> · <a href="README-tr.md">🇹🇷 Türkçe Oku</a>
</p>

<p align="center">
  <img src="predator-sense-gui/resources/logo.jpeg" width="120" alt="Predator Sense Logo">
</p>

<p align="center">
  <b>Módulo de kernel Linux no oficial y GUI para el control de hardware de portátiles gaming Acer</b><br>
  <i>Retroiluminación RGB del Teclado &bull; Modo Turbo &bull; Monitoreo de Temperatura &bull; Perfiles de Rendimiento</i>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Idioma-Rust-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/GTK-4-blue?logo=gtk" alt="GTK4">
  <img src="https://img.shields.io/badge/Userspace-100%25_Rust-orange?logo=rust" alt="Userspace 100% Rust">
  <img src="https://img.shields.io/badge/Licencia-GPL--3.0-green" alt="License">
  <img src="https://img.shields.io/badge/Plataforma-Linux-yellow?logo=linux" alt="Linux">
</p>

<p align="center">
  Creado y mantenido por <a href="https://github.com/cleyton1986">Cleyton Alves</a>
</p>

---

## Descargo de responsabilidad

> **Advertencia**
> **¡Úsalo bajo tu propia responsabilidad!** Este es un proyecto **no oficial**. Acer no participó en su desarrollo. El módulo del kernel se desarrolló mediante ingeniería inversa de la aplicación oficial de Windows PredatorSense. Este controlador interactúa con métodos WMI/ACPI de bajo nivel que no han sido probados en todas las series de portátiles. Los autores no se responsabilizan de ningún daño a tu hardware.

> **Nota**
> Todas las marcas comerciales, nombres de productos y logotipos mencionados (Acer, Predator, PredatorSense, Helios, Nitro, AeroBlade, CoolBoost) son propiedad de sus respectivos titulares (Acer Inc.). Este proyecto no está afiliado, avalado ni patrocinado por Acer Inc. de ninguna manera.

> **Imágenes de productos**
> Las fotos de los portátiles en `predator-sense-gui/resources/models/` muestran productos oficiales de Acer Predator/Nitro y se usan únicamente para que la app identifique visualmente el modelo detectado en la propia máquina del usuario (comparándolo con el `product_name` reportado por la DMI/BIOS del sistema). Estas imágenes **no están cubiertas por la licencia GPLv3 de este proyecto** — los derechos de autor de las fotografías del producto pertenecen a Acer Inc. y/o sus creadores originales. Se incluyen aquí de buena fe, sobre una base no comercial y puramente informativa (uso nominativo/de identificación de producto), sin ninguna reivindicación de propiedad por parte de este proyecto. Si eres el titular de los derechos y deseas que se elimine una imagen, abre un issue y se retirará con prontitud.

Esta aplicación fue creada para **uso personal**, para sacar el máximo provecho de un portátil gaming Acer en Linux, ya que Acer no ofrece soporte oficial de Linux para PredatorSense. Se comparte libremente para quien quiera lo mismo.

Si esta app/proyecto te ayudó y/o te gustó de alguna manera, considera dejar una estrella, ayuda mucho ⭐

---

## Capturas de pantalla

<p align="center"><b>Dashboard</b> — Foto del portátil y especificaciones completas del sistema de un vistazo: CPU, GPU, RAM, almacenamiento, red y SO.</p>
<p align="center"><img src="assets/psense-1.png" width="800" alt="Dashboard"></p>

<p align="center"><b>Temperaturas</b> — Medidores en tiempo real para CPU, GPU, sistema, unidades NVMe, WiFi y RAM, todo en una sola pantalla.</p>
<p align="center"><img src="assets/psense-2.png" width="800" alt="Temperaturas"></p>

<p align="center"><b>Uso</b> — CPU, GPU, memoria y almacenamiento con los procesos principales, barras animadas y detalles que se expanden al hacer clic (con una animación de fuego estilo CSS en el medidor de temperatura).</p>
<p align="center"><img src="assets/psense-3.png" width="800" alt="Uso"></p>

<p align="center"><b>Red</b> — Gráficos de descarga/subida en tiempo real con seguimiento de picos y detección automática de interfaz (Wi-Fi o Ethernet).</p>
<p align="center"><img src="assets/psense-4.png" width="800" alt="Red"></p>

<p align="center"><b>Iluminación</b> — Colores estáticos por zona (4 secciones) y efectos RGB dinámicos del teclado (Breathing, Neon, Wave, Shifting, Zoom).</p>
<p align="center"><img src="assets/psense-5.png" width="800" alt="Iluminación"></p>

<p align="center"><b>Modos</b> — Perfiles de rendimiento: Silencioso, Equilibrado, Rendimiento y Turbo, más un nivel Eco exclusivo para batería (CPU governor + Intel EPP + límite de potencia de la GPU).</p>
<p align="center"><img src="assets/psense-6.png" width="800" alt="Modos"></p>

<p align="center"><b>GameSync</b> — Registra un juego y su perfil; la app cambia a él automáticamente mientras el juego está en ejecución y restaura lo que estuviera activo antes en cuanto se cierra.</p>
<p align="center"><img src="assets/psense-15.png" width="800" alt="GameSync"></p>

<p align="center"><b>Control de Ventiladores</b> — RPM en tiempo real con ventiladores animados girando, interruptor de CoolBoost y modos Auto/Max.</p>
<p align="center"><img src="assets/psense-7.png" width="800" alt="Control de Ventiladores"></p>

<p align="center"><b>Batería</b> — Porcentaje de carga, voltaje, corriente, potencia, ciclos, salud, fabricante y límite de carga al 80% para mayor longevidad.</p>
<p align="center"><img src="assets/psense-8.png" width="800" alt="Batería"></p>

<p align="center"><b>GPU</b> — Panel NVIDIA con gráficos en tiempo real, frecuencias, utilización, VRAM, consumo de energía e información PCIe.</p>
<p align="center"><img src="assets/psense-9.png" width="800" alt="GPU"></p>

<p align="center"><b>Gráficos</b> — Gráficos históricos detallados de CPU y GPU con seguimiento de mínimos/máximos.</p>
<p align="center"><img src="assets/psense-10.png" width="800" alt="Gráficos"></p>

<p align="center"><b>Asistente de IA (beta)</b> — Asistente de IA local basado en Ollama: chat, gestor de modelos (lista los modelos instalados, descarga nuevos, elige cuál se ejecuta), uso de recursos VRAM/GPU en tiempo real mientras piensa, y un registro de acciones persistente.</p>
<p align="center"><img src="assets/psense-11.png" width="800" alt="Asistente de IA"></p>

<p align="center"><b>Controladores y manuales</b> — Muestra el número de serie (con un botón para copiarlo) y un enlace directo a la página oficial de controladores y manuales de Acer, además de una ilustración de dónde encontrar la etiqueta del número de serie en el portátil.</p>
<p align="center"><img src="assets/psense-16.png" width="800" alt="Controladores y manuales"></p>

<p align="center"><b>Ajustes</b> — Minimizar a la bandeja, iniciar al arrancar, aplicar automáticamente el perfil al inicio, preferencias de idioma y lista de funciones compatibles por modelo.</p>
<p align="center"><img src="assets/psense-12.png" width="800" alt="Ajustes"></p>

<p align="center"><b>Iluminación del logo de la tapa</b> — Control RGB independiente para el logo en la parte trasera de la pantalla, en modelos con un logo de tapa compatible con color (Static/Breathing/Neon). Detectado en tiempo de ejecución: el control solo aparece si el hardware responde a una prueba de capacidades, por lo que permanece oculto de forma segura en los modelos que no lo tienen.</p>
<p align="center"><img src="assets/psense-13.png" width="800" alt="Iluminación del logo de la tapa"></p>
<p align="center"><img src="assets/psense-14.jpg" width="800" alt="Logo de la tapa encendido en verde en un Predator PHN16-73"></p>
<p align="center"><sub>Funcionalidad aportada por <a href="https://github.com/jlucaso1">@jlucaso1</a>, probada en su propio Predator PHN16-73. El logo de la tapa de este portátil no admite color, así que la funcionalidad se verificó usando su hardware.</sub></p>

---

## Acerca de

Módulo de kernel Linux no oficial para la retroiluminación RGB del teclado y el modo Turbo de portátiles gaming Acer (Acer Predator, Acer Helios, Acer Nitro).

Inspirado en y basado en el proyecto [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) de [JafarAkhondali](https://github.com/JafarAkhondali) y colaboradores. Este proyecto extiende el módulo de kernel Linux Acer-WMI existente para soportar las funciones gaming de Acer, y añade una **aplicación de escritorio GUI completa** construida con Rust y GTK4.

---

## Funciones

| Función | Descripción |
|---------|-------------|
| **Dashboard** | Foto del portátil + especificaciones completas del sistema (CPU, GPU, RAM, almacenamiento, red, SO) |
| **Temperaturas** | Medidores en tiempo real para CPU, GPU, sistema, NVMe, WiFi y RAM |
| **Uso** | Vista de 4 pestañas: CPU / GPU / Memoria / Almacenamiento con los procesos principales, detalles que se expanden al hacer clic y animación de fuego estilo CSS en los medidores de temperatura |
| **Red** | Gráficos de descarga/subida en tiempo real con seguimiento de picos y detección automática de interfaz |
| **Control RGB del Teclado** | Colores estáticos por zona (4 zonas) y efectos dinámicos (Breathing, Neon, Wave, Shifting, Zoom) vía WMI. En hardware sin el módulo del kernel, el RGB funciona en su lugar de forma nativa vía USB/I2C-HID — chip ENEK5130 (estático de 4 zonas, Breathing/Neon), chip Sunrex 2024+ (zona única, lista completa de efectos) o chip Chicony (paleta de 7 colores, Helios 300) — detectado automáticamente, ver [Compatibilidad](#compatibilidad) |
| **Logo RGB de la Tapa** | Controles independientes de encendido, color sólido, brillo, Breathing y Neon para el emblema en la parte trasera de la pantalla, con vista previa vectorial en tiempo real. Solo se muestra tras la detección de capacidades HID en tiempo de ejecución |
| **Perfiles de Rendimiento** | Modos Silencioso / Equilibrado / Rendimiento / Turbo, más un nivel Eco exclusivo para batería (CPU governor + Intel EPP + límite de potencia de la GPU) |
| **Control de Ventiladores** | RPM en tiempo real con ventiladores animados girando, interruptor de CoolBoost, modos Auto/Max, más control experimental de PWM por ventilador y curva de temperatura automática (donde esté soportado) |
| **Batería** | Estadísticas de carga, ciclos, salud, información del fabricante y límite de carga al 80% para mayor longevidad |
| **Panel GPU** | Métricas NVIDIA: temperatura, utilización, VRAM, frecuencias, consumo de energía, información PCIe con gráficos en tiempo real, más un **control deslizante de límite de potencia (TGP)** |
| **Gráficos** | Gráficos históricos detallados de CPU y GPU con seguimiento de mínimos/máximos |
| **Asistente de IA** 🧪 | Asistente de IA local y opcional basado en [Ollama](https://ollama.com) — lee el estado del hardware en tiempo real y sugiere o aplica cambios a través de un conjunto fijo de acciones ya validadas (perfil térmico, modo de ventilador, CoolBoost, RGB, límite de potencia de la GPU, batería). Chat, gestor de modelos (descargar/seleccionar), monitor de recursos/VRAM en tiempo real y un registro de acciones persistente. Aplicación automática o confirmación siempre requerida, tú eliges. Requiere Ollama instalado por separado — ver [Asistente de IA](#asistente-de-ia-beta) más abajo |
| **Detección automática de capacidades** | Detecta lo que soporta cada modelo y adapta la interfaz — las funciones no soportadas se muestran como "no disponible en este modelo" en lugar de generar un error. Las funciones soportadas se listan en Ajustes |
| **Alertas de temperatura** | Notificación de escritorio cuando la CPU/GPU superan los 90°C (funciona también desde la bandeja) |
| **Perfil de energía automático** | Cambia de perfil automáticamente al pasar entre corriente alterna y batería — el perfil de destino para cada estado es configurable en Ajustes (predeterminado: Rendimiento con corriente, Equilibrado con batería) |
| **Registro de depuración** | Interruptor opcional en Ajustes — registra los eventos del daemon y de la app en `~/.local/share/predator-sense/` (rotativo, 5MB×3) para solución de problemas remota. Desactivado por defecto |
| **Bandeja del Sistema** | Minimizar a la bandeja con el icono de Predator — la app permanece activa en segundo plano |
| **Tecla PredatorSense** | Mapeo de tecla de hardware — la tecla junto al NumLock abre la app |
| **DKMS** | Los módulos del kernel se recompilan automáticamente en cada actualización del kernel |
| **Internacionalización** | Inglés / Portugués automático según el idioma del sistema |
| **Interfaz Gaming** | Tema oscuro con barras neón pulsantes, medidores circulares discontinuos, bordes de panel poligonales. El color de acento sigue automáticamente la marca detectada — cian en Predator/Helios/Triton, naranja/rojo en Nitro (a juego con NitroSense) — sin ningún ajuste que activar |

---

## Compatibilidad

**¿Funcionará esto en mi portátil?**

Leyenda: ✅ probado y funcionando · 🟡 implementado, no probado (necesita un tester) · 🧪 experimental (necesita un tester) · ❌ no funciona · `-` no aplica

| Nombre del Producto | Turbo (Impl.) | Turbo (Probado) | RGB (Impl.) | RGB (Probado) | Lectura RPM ventiladores | Perfiles de ventiladores | Fan PWM % |
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

> Si tu modelo no está en la lista, puede que aun así funcione — el módulo del kernel detecta automáticamente las interfaces WMI compatibles. Si te funcionó (o no) por favor abre un issue mencionando tu modelo para que podamos actualizar esta tabla.

### Control de ventiladores — tres niveles

| Nivel | Qué hace | Disponibilidad |
|---|---|---|
| **Lectura RPM de ventiladores** | Lee la velocidad del ventilador de CPU/GPU (`fan1_input`, `fan2_input`) | La mayoría de modelos gaming (detectado automáticamente) |
| **Perfiles de ventiladores** | Silencioso / Equilibrado / Rendimiento / Turbo vía `platform_profile` | Modelos `predator_v4` |
| **Fan PWM %** 🧪 | Control de velocidad por ventilador (`pwm1`/`pwm2` 0–100%) portado desde `acer-wmi` mainline vía WMI — **solo kernel ≥ 6.14** | Subconjunto de modelos con `ACER_CAP_PWM` (AN515-58, PHN16-72/73, PH16-72, …) |

> **🧪 El control PWM de ventiladores es experimental.** Está portado desde el driver `acer-wmi` del kernel Linux upstream y usa métodos WMI seguros (sin escrituras EC crudas), pero **no ha sido verificado en hardware real** por el mantenedor (que posee un PH315-54, que no tiene PWM). Si tienes un modelo compatible, los reportes de pruebas son muy bienvenidos. **Úsalo bajo tu propia responsabilidad** — ver el descargo de responsabilidad al inicio.

### Alternativa: linuwu_sense (hardware sin quirk, con Turbo que no funciona)

El fallback `enable_all=1` de `facer` reconoce cualquier placa compatible con WMI de Acer, pero el conjunto completo de perfiles `predator_v4` (5 perfiles incluyendo `balanced-performance`/`performance`, `turbo_state` escribible) solo se aplica a las placas presentes en su tabla de quirks DMI. En una placa sin quirk, `platform_profile_choices` se limita a `low-power quiet balanced` y `turbo_state` permanece de solo lectura aunque el firmware soporte más — reportado en una unidad PHN16-73 (Macan_ARX, BIOS V1.26) en [#33](https://github.com/cleyton1986/predator-sense/issues/33).

Si ese es tu caso, el módulo de la comunidad [Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense) (cargado con `predator_v4=1`) expone el conjunto completo de perfiles a través de las mismas interfaces genéricas `platform_profile`/`intel_pstate`/`acer-wmi-battery` que esta app ya lee directamente — sin que intervenga ninguna ruta de código específica de `facer`. Desde `v0.2.71-preview` la app detecta `linuwu_sense` y omite el aviso de "instalar facer" cuando es ese driver el que realmente proporciona esas interfaces. El RGB y la calibración del perfil térmico (ambos exclusivos de `facer`, ver arriba y abajo) todavía necesitan `facer` en sí y permanecen no disponibles con linuwu_sense.

### RGB sin el módulo del kernel (solo hardware I2C-HID)

Algunos modelos (confirmados: PHN16S-71, PHN16-73, AN16S-61) enrutan el controlador RGB del teclado a través de un chip I2C-HID separado (ENEK5130) en lugar de la interfaz WMI de `facer.ko` — la app se comunica con él directamente vía `/dev/hidrawN`, por lo que estos funcionan incluso si el módulo del kernel no está cargado en absoluto:

| Función | Estado |
|---|---|
| Color estático por zona, brillo, apagado de retroiluminación | ✅ confirmado funcionando (PHN16S-71, AN16S-61) |
| Efectos dinámicos — Breathing, Neon | ✅ confirmado funcionando (PHN16S-71, AN16S-61) — nativo, una sola escritura HID, el hardware repite el patrón en bucle por sí solo. En la unidad PHN16S-71, Breathing ignora el color elegido y cicla el arcoíris en su lugar; puede variar en otro hardware |
| Efectos dinámicos — Wave, Shifting, Zoom | Solo vista previa en pantalla (sin escrituras al hardware) — se descubrió que los códigos de estos efectos significan cosas distintas entre generaciones de hardware, así que todavía no están implementados |
| Logo RGB de la tapa — apagado, color sólido, brillo, Breathing, Neon | ✅ confirmado funcionando (PHN16-73) |

El soporte del logo de la tapa no se habilita mediante una lista de nombres de modelo permitidos. El controlador debe anunciar el target `0x83` en su reporte A1 de targets y devolver capacidades A3 coincidentes y no vacías antes de que se muestre la interfaz; la app repite esa verificación inmediatamente antes de cada escritura. El daemon de hotkey solo restaura un ajuste que la app haya aplicado previamente con éxito después del login y de la reanudación, y omite el logo por completo cuando no hay ningún ajuste guardado o el target no está presente.

Un [reporte independiente sobre el AN16S-61](https://github.com/cleyton1986/predator-sense/issues/31) (ver también la [herramienta de protocolo independiente](https://github.com/ArnarValur/Nitro16S-AI-RGB-Keyboard) del propio autor del reporte) mapeó seis modos de cable nativos más además de static/Breathing/Neon/Wave (un modo de apagado por hardware, un modo de parpadeo de arranque que el propio EC dispara, y cuatro animaciones incorporadas más), además de un target LED de tecla de modo/turbo. Nada de eso está todavía implementado en la app — se necesita antes una ranura definida para códigos de efecto nativos del hardware, así que queda registrado como una mejora futura.

El mismo reporte también incluyó un report descriptor HID decodificado, extraído directamente del controlador, que resolvió un bug real: la app estaba leyendo el conteo de zonas del reporte de capacidades A3 desde el byte equivocado (`byte[3]`, una constante fija por clase de target) en lugar del byte que el propio descriptor del controlador declara para ello (`byte[4]`). Corregido en `v0.2.69-preview` tanto en la app como en el daemon de hotkey. Esta es una corrección a nivel de protocolo, no un cambio por modelo - el diseño de campos del report descriptor proviene del propio firmware del chip (el mismo chip `0CF2:5130` en los tres modelos confirmados) - y no cambia ningún byte en el cable en hardware ya confirmado funcionando, ya que el valor anterior era siempre un superconjunto sobre-inclusivo del correcto.

### RGB en hardware 2024+ (Sunrex/Darfon USB HID)

Una generación más nueva (PH16-72 y otros modelos de 2024-2026 que comparten los mismos chips USB HID, ver issue #26) trasladó el RGB del teclado y del logo de la tapa fuera de WMI *y* fuera del chip ENEK5130 anterior, a un par de controladores completamente distintos — Sunrex `05af:*` para el teclado, Darfon `0d62:*` para el logo. La app también detecta y controla estos directamente, seleccionados automáticamente sobre las rutas ENEK5130/WMI siempre que estén presentes:

| Función | Estado |
|---|---|
| Teclado: Off, Static, Breathing, Wave, Snake, Neon, Spot, Star, Rainbow, 5× Slash, Zoom, Row Wave, Swiping | 🟡 implementado, a la espera de confirmación en hardware real |
| Logo de la tapa: apagado, color sólido, brillo, Breathing | 🟡 implementado, a la espera de confirmación en hardware real |

Este chip no tiene zonas independientes — todo el teclado usa un solo color/efecto a la vez, a diferencia del controlador ENEK5130 de 4 zonas anterior. El protocolo de cable se realizó mediante ingeniería inversa byte por byte a partir de dos versiones descompiladas de la app oficial de Windows (cada secuencia de bytes fija y fórmula de checksum coincidían exactamente entre ambas), no se adivinó — pero nadie lo ha confirmado todavía contra hardware físico, así que trátalo como no probado hasta que llegue un reporte real.

Un tercer chip (Chicony, Helios 300/PH317-56) usa otro protocolo USB HID distinto, documentado mediante ingeniería inversa de la comunidad ([NT411/Acer-Predator-Fan-RGB-Controller-Linux](https://github.com/NT411/Acer-Predator-Fan-RGB-Controller-Linux)) y reimplementado aquí a partir de esa especificación — paleta fija de 7 colores (una limitación de hardware/firmware, no RGB arbitrario) en 12 efectos. También 🟡, a la espera de confirmación.

### ¿Ya usas Linuwu-Sense o DAMX?

[Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense) (y [DAMX](https://github.com/PXDiv/Div-Acer-Manager-Max), que está construido sobre él) es un proyecto separado y no relacionado que también controla hardware Acer Predator/Nitro en Linux. No es una dependencia de este proyecto y ninguno de su código se usa aquí — pero su módulo del kernel se vincula a los **mismos GUID de WMI** que necesita `facer`, y el kernel no permite que dos drivers reclamen el mismo dispositivo a la vez.

Si el instalador detecta que `linuwu_sense` ya está cargado o instalado vía DKMS, automáticamente **deja intacta tu configuración existente** — no pone en la lista negra a `acer_wmi` ni fuerza la carga de `facer`, así que no entra en conflicto (ni rompe) una instalación de Linuwu-Sense/DAMX que ya funciona. El RGB del teclado sigue funcionando a través de esta app por la ruta HID (ver arriba) sin importar qué driver de plataforma esté activo; en ese caso, el control de ventiladores/térmico se queda con la herramienta que ya usabas para gestionarlo.

---

## Instalación

### Instalador Precompilado (Más Rápido)

Descarga el instalador de la release directamente y ejecútalo:

```console
curl --fail --location https://github.com/cleyton1986/predator-sense/releases/latest/download/predator-sense-installer --output predator-sense-installer
chmod +x predator-sense-installer
sudo ./predator-sense-installer --install
```

El instalador, el helper privilegiado, el listener de hotkey y el servicio de bandeja son provistos todos por el mismo binario multicall de Rust. El instalador descarga y configura todo sin necesidad de un bootstrap por script de shell.

### Instalador Interactivo (binario precompilado, no necesita toolchain de Rust)

Descarga el binario `predator-sense-installer` desde la página de [Releases](../../releases). Es un binario de Rust independiente, no un paquete — igual necesita acceso a internet para obtener el código fuente de la app (para el módulo del kernel) y el binario precompilado de la release correspondiente, pero evita por completo instalar Rust y compilar la app GTK4 en tu máquina:

```console
chmod +x predator-sense-installer
sudo ./predator-sense-installer
```

Selecciona la **opción 1** (Instalación Completa). El instalador automáticamente:

1. Detecta tu distribución (Debian/Ubuntu/Mint, Fedora, Arch)
2. Instala las dependencias del sistema (GTK4, libadwaita, herramientas de compilación, headers del kernel)
3. Descarga el código fuente + binario precompilado de la release correspondiente
4. Compila y carga el módulo del kernel `facer` (esta parte siempre se compila localmente — los módulos del kernel no se pueden distribuir precompilados entre distintas versiones del kernel)
5. Crea una entrada en el menú de aplicaciones con icono
6. Mapea la tecla de hardware PredatorSense (inicio automático al iniciar sesión)
7. Configura el soporte de bandeja del sistema

La ruta precompilada no necesita Rust/cargo en la máquina de destino. El instalador también se copia a `/opt/predator-sense/` como herramienta de gestión independiente para comprobaciones de estado, recarga del módulo del kernel, actualizaciones y desinstalación (ver [Opciones del Instalador](#opciones-del-instalador)).

Después de la instalación, abre la app de una de estas formas:
- Presionando la **tecla PredatorSense** (junto al NumLock)
- Buscando **"Predator Sense"** en tu menú de aplicaciones
- Ejecutando `/opt/predator-sense/predator-sense` en una terminal

### Instalación Manual (Compilación desde el código fuente)

#### Requisitos previos

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

**Rust** (si no está instalado):
```console
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### Compilación e Instalación

```console
# Clona el repositorio
git clone https://github.com/cleyton1986/predator-sense.git
cd predator-sense/predator-sense-gui

# Compila la GUI y el instalador/servicios de Rust
cargo build --release
cargo build --release --manifest-path installer/Cargo.toml

# Instala la compilación local y registra las fuentes C existentes del kernel en DKMS
sudo installer/target/release/predator-sense-installer --install

# Ejecutar
/opt/predator-sense/predator-sense
```

---

## Uso

### RGB del Teclado

1. Ve a **Iluminación** en la barra lateral
2. Elige **Estático** (colores por zona) o **Dinámico** (efectos)
3. **Modo estático:** ajusta los deslizadores R/G/B para cada una de las 4 secciones del teclado
4. **Modo dinámico:** selecciona un efecto (Breathing, Neon, Wave, Shifting, Zoom) y ajusta la velocidad
5. Haz clic en **Aplicar**

> En hardware solo I2C-HID sin el módulo del kernel (ver [Compatibilidad](#compatibilidad)), Breathing y Neon animan de verdad; Wave/Shifting/Zoom muestran solo una vista previa en pantalla, claramente etiquetada como tal — el teclado físico todavía no cambia para esos.

### Logo RGB de la Tapa

1. Ve a **Iluminación** y selecciona **Logo de la tapa** (el selector solo aparece cuando se detecta un target HID compatible)
2. Usa **Iluminación** para encender o apagar el emblema
3. Elige **Estático**, **Breathing** o **Neon**, luego ajusta los controles disponibles de color, brillo y velocidad mientras revisas la vista previa en tiempo real
4. Haz clic en **Aplicar al logo**

El último estado aplicado con éxito se restaura cuando arranca el servicio de hotkey del usuario y tras suspender/hibernar. Los colores de los efectos animados están controlados por el firmware, así que la vista previa representa su comportamiento en lugar de ofrecer un selector de color para esos modos.

> El firmware controla la animación de iluminación que se muestra antes de que Linux inicie el servicio de usuario. Un estado "apagado" guardado se restaura después del login, pero esta app no puede suprimir la animación anterior de BIOS/arranque.

### Perfiles de Rendimiento

En sistemas con Intel P-State + HWP activos, el lado de la CPU se resuelve así:

| Perfil | Política HWP | Intel EPP | Rendimiento mín. | Potencia GPU | Ventilador | Caso de Uso |
|---------|------------|-----------|------------------|-----------|-----|----------|
| **Eco**⁴ | powersave | power | 5% | 25W³ | Auto | Máxima autonomía de batería |
| **Silencioso** | powersave | power | 10% | 40W³ | Auto | Trabajo silencioso |
| **Equilibrado** | powersave | balance_performance | 17% | 80W³ | Auto | Uso general |
| **Rendimiento** | powersave¹ | performance | 50% | 100W³ | Max | Gaming |
| **Turbo** | performance² | 0 (forzado por el kernel) | 100% | 110W³ | Max | Rendimiento máximo |

Seleccionar cualquier perfil también aplica su modo de ventilador - no se necesita un paso separado.
Elegir Rendimiento o Turbo lleva el ventilador a Max (igual que la tecla física
Turbo); Silencioso, Equilibrado y Eco lo dejan en Auto.

⁴ Exclusivo de batería, igual que la app oficial de Windows: nunca ofrece Eco
como opción con corriente en absoluto, así que la tarjeta solo aparece en la
página de Modo mientras está desconectado. No existen cifras oficiales
confirmadas de Acer de vatiaje/EPP para este nivel, así que sus ajustes son
una extrapolación conservadora por debajo de los propios valores de
Silencioso, no un valor medido como los otros cuatro.

¹ La política HWP `powersave` de Intel P-State es un algoritmo de escalado
dinámico, no el governor genérico de frecuencia mínima. Mantiene escribible
el EPP nominal específico del modelo, haciendo de Rendimiento un nivel
dinámico del 50% hasta el máximo.

² La propia política HWP `performance` fuerza el EPP a 0 y restringe el rango
de P-state disponible a su límite superior. Predator Sense se apoya en ese
comportamiento del kernel en lugar de requerir escrituras numéricas de EPP.
El backend se detecta a partir de cada política cpufreq, sin una lista de
modelos de CPU permitidos. Otros drivers mantienen el mapeo existente
`performance` + EPP nominal `performance`, y los sistemas sin EPP omiten
solo ese control opcional.

³ Best-effort vía `nvidia-smi -pl`, igual que el control deslizante de límite
de potencia del Panel GPU más abajo - se omite silenciosamente si
`nvidia-smi` no está presente, y en algunos portátiles la vBIOS nunca expone
el control de límite de potencia de NVML (`nvidia-smi -q` reporta `Power
Management Object: N/A`, todo valor de `-pl` es rechazado sin importar lo
que se solicite). Eso es un límite a nivel de firmware, no algo que esta app
- ni ningún software de Linux - pueda cambiar; subirlo significa flashear
una vBIOS distinta con una herramienta exclusiva de Windows como `nvflash`,
un riesgo real de dejar inutilizada la GPU y es enteramente decisión del
propietario.

**Diferencia conocida respecto a la app oficial de Windows:** en Silencioso,
el PredatorSense oficial también activa el Whisper Mode de NVIDIA
(`NvAPI_NvToppsJpacSetControl`), que limita la tasa de fotogramas a 60 FPS
para que la curva del ventilador funcione más silenciosa. Ese control es
parte de la API de driver de NVIDIA exclusiva de Windows y no tiene
equivalente en Linux, así que Silencioso aquí no es tan silencioso bajo
carga como el Silencioso de Windows en el mismo hardware - esto es una
limitación de la plataforma, no un bug de esta app.

### Perfiles de Potencia del Firmware (medidos, no adivinados)

Todo lo de la tabla de arriba solo redistribuye un presupuesto de potencia
existente entre CPU y GPU. **El límite de potencia del paquete en sí** lo
establece el propio perfil térmico del firmware, y en algunos modelos el
firmware arranca en el más bajo — así que ningún cambio de governor, EPP o
`min_perf` sube el techo ni un solo vatio.

`platform_profile` no siempre puede alcanzar esos modos. El driver del
kernel los nombra a partir de una tabla fija (`BALANCED=0, QUIET=1,
PERFORMANCE=2, TURBO=3, ECO=4`) que no se cumple en todos los firmwares.
Medido en un Predator PHN16-73 (Arrow Lake, BIOS V1.26), escribiendo cada
índice crudo y releyendo el límite del paquete:

| Índice de firmware | Sostenido (PL1) | Ráfaga (PL2) | Nombre vía `platform_profile` |
|---:|---:|---:|---|
| 6 | 45 W | 50 W | *(ninguno — inalcanzable)* |
| 0 | 55 W | 160 W | `balanced` |
| 1 | 70 W | 160 W | `quiet` |
| 4 | 95 W | 160 W | `low-power` |
| 5 | **115 W** | 160 W | *(ninguno — inalcanzable)* |

Los modos más fuerte y más débil no tienen ningún nombre, y los tres que sí
lo tienen están etiquetados en el orden equivocado. Codificar a mano una
tabla corregida solo trasladaría el problema al siguiente firmware, así que
Predator Sense **mide en su lugar**:

1. El módulo del kernel expone el índice crudo y la máscara de bits de
   índices soportados por el propio firmware como
   `/sys/devices/platform/acer-wmi/thermal_profile` y
   `thermal_profile_supported`.
2. **Modo → Calibrar perfiles** escribe cada índice soportado y lee el
   límite de paquete resultante desde `intel-rapl-mmio`, luego los ordena
   por potencia sostenida. Tarda unos segundos y mueve audiblemente los
   ventiladores mientras se ejecuta.
3. A partir de entonces los cuatro niveles de arriba también controlan el
   perfil del firmware, anclados de forma que Silencioso caiga en el
   realmente más débil y Turbo en el realmente más fuerte.

Notas:

- **Las máquinas sin RAPL legible** (modelos AMD, Intel más antiguos) no
  se pueden ordenar. Los perfiles siguen listados y son seleccionables a
  mano, pero los cuatro niveles deliberadamente dejan en paz al firmware en
  lugar de adivinar un orden — en el firmware de arriba, adivinar por índice
  pondría a Turbo en el perfil de 45 W.
- El firmware **olvida** el perfil en cada ciclo de energía, así que el
  servicio de arranque reaplica el último que elegiste.
- En modelos donde el firmware ata la iluminación del teclado al modo de
  energía, cada cambio — incluido cada paso de una calibración — repinta el
  teclado. Eso lo hace el firmware, no esta app; vuelve a aplicar tus
  colores desde la página de Iluminación después si te molesta.
- La **tecla física de cambio de modo** recorre el mismo orden medido; ver
  abajo.

### Tecla Física de Cambio de Modo

Algunos modelos tienen una tecla dedicada que recorre los modos de energía.
Se reporta **únicamente** como un reporte HID de entrada crudo en el
embedded controller y no genera ningún evento del subsistema de entrada en
absoluto, por lo que parece muerta en Linux mientras que la tecla
PredatorSense (un hotkey WMI) funciona.

El daemon vigila el dispositivo HID del EC de Acer para detectarla. Los
valores predeterminados se capturaron en un PHN16-73 (`1025:174B`, reporte
`04 85 ff`); se espera que otros modelos difieran, así que ambos se pueden
sobrescribir sin necesidad de recompilar:

`~/.config/predator-sense/mode_key.json`:

```json
{ "product": "0000ABCD", "report": [4, 133, 255] }
```

(JSON estricto — un comentario `//` en ese archivo lo vuelve imposible de
analizar, y el daemon recae en los valores predeterminados dejando una nota
en su log.)

Si tu tecla no hace nada, el daemon registra en el log cada dispositivo HID
de Acer que encontró al iniciar (activa `debug_logging` en Ajustes).
Encuentra el correcto con `sudo hexdump -C /dev/hidrawN` mientras presionas
la tecla, luego apunta el archivo hacia él — y por favor abre un issue con
los valores para que puedan enviarse como predeterminados para tu modelo.

El firmware también se niega a cambiar de modo por debajo del 40% de
batería; el daemon reporta eso en lugar de dejar que la tecla parezca rota.

### Perfil automático según la fuente de alimentación

Cuando está habilitado en Ajustes (activado por defecto en instalaciones
nuevas), esto no es solo una reacción a conectar/desconectar - se aplica de
forma continua:
- **Con corriente:** siempre Rendimiento o Turbo. Si uno de esos dos ya está
  activo, se deja como está - el cambio automático nunca contradice una
  elección manual entre ambos.
- **Con batería:** siempre Equilibrado o Silencioso, nunca
  Rendimiento/Turbo. Por debajo del 15% de batería, se fuerza Silencioso sin
  importar el objetivo configurado.

### Panel de GPU

Monitoreo NVIDIA GPU en tiempo real:
- Temperatura, utilización, uso de VRAM, consumo de energía (medidores circulares)
- Gráficos históricos en tiempo real de temperatura y utilización (ventana de 2 minutos)
- Frecuencia de núcleo, frecuencia de memoria, P-State, información de enlace PCIe, versión de VBIOS

### Asistente de IA (beta)

Un asistente de IA local y opcional, basado en [Ollama](https://ollama.com) ejecutándose enteramente en tu máquina — nada se envía a ningún lado.

1. Instala Ollama por separado siguiendo sus [instrucciones oficiales para Linux](https://ollama.com/download/linux)
2. Ve a **AI** en la barra lateral y descarga un modelo desde el gestor de modelos integrado (`smollm2:1.7b` o superior — los modelos más pequeños no soportan de forma confiable el tool-calling)
3. Activa el asistente en **Ajustes** y elige **Aplicar automáticamente** (aplica las sugerencias de inmediato) o **Confirmar siempre** (predeterminado — cada cambio sugerido espera tu aprobación)

El asistente lee el estado del hardware en tiempo real (temperatura, ventilador, perfil térmico, batería) y puede sugerir o aplicar cambios a través de un conjunto fijo de acciones ya validadas — nunca toca directamente el hardware/EC crudo, y cada acción corresponde 1:1 con una función que esta app ya usaba antes de que existiera la función de IA. El modelo se carga solo para ejecutar un análisis, luego se descarga — no se queda inactivo en memoria. Toda la actividad de IA se registra en un log de acciones persistente y revisable en la misma página.

---

## Opciones del Instalador

El instalador de Rust ofrece una TUI interactiva:

```console
sudo ./predator-sense-installer              # Menú interactivo
sudo ./predator-sense-installer --install    # Instalación completa directa
sudo ./predator-sense-installer --uninstall  # Elimina todo
sudo ./predator-sense-installer --reload-module # Recompila/recarga el módulo del kernel
sudo ./predator-sense-installer --status     # Muestra el estado de los componentes
```

---

## Desinstalación

```console
sudo ./predator-sense-installer  # Selecciona la opción 2
```

O manualmente:
```console
pkill -f "/opt/predator-sense/predator-sense"
sudo rm -rf /opt/predator-sense
sudo rm -f /usr/share/applications/predator-sense.desktop
sudo rm -f /usr/share/icons/hicolor/128x128/apps/predator-sense.png
rm -f ~/.config/systemd/user/predator-sense-hotkey.service
rm -f ~/.config/autostart/predator-sense-hotkey.desktop
sudo rmmod facer  # Opcional: descargar el módulo del kernel
```

---

## Solución de Problemas

<details>
<summary><b>El RGB del teclado no cambia / atascado en un efecto</b></summary>

El estado del módulo del kernel puede haberse atascado. Recárgalo:
```console
sudo rmmod facer
sudo insmod /path/to/kernel/facer.ko
# O usa el instalador: sudo ./predator-sense-installer → Opción 4
```
</details>

<details>
<summary><b>El módulo no carga</b></summary>

```console
# Comprueba que el dispositivo WMI existe
ls /sys/bus/wmi/devices/7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56/

# Comprueba los logs del kernel
sudo dmesg | grep -i facer

# Asegúrate de que los headers coinciden con tu kernel
sudo apt install linux-headers-$(uname -r)
```
</details>

<details>
<summary><b>La tecla PredatorSense no funciona</b></summary>

```console
# Comprueba el servicio de hotkey de Rust
systemctl --user status predator-sense-hotkey.service
pgrep -af predator-sense-hotkey

# Asegúrate de que el usuario está en el grupo 'input' (se requiere cerrar sesión e iniciarla de nuevo por completo, o reiniciar, tras añadirlo)
groups | grep input
sudo usermod -aG input $USER
```
</details>

<details>
<summary><b>La página de GPU NVIDIA no muestra datos</b></summary>

```console
# Verifica que nvidia-smi funciona
nvidia-smi
# Si no, instala los drivers propietarios de NVIDIA
```
</details>

<details>
<summary><b>Mi modelo no tiene un quirk correspondiente (faltan perfiles/lectura de ventilador/PWM)</b></summary>

Si tu modelo exacto todavía no está en la lista de compatibilidad, intenta forzar todas las funciones opcionales de la familia `predator_v4` y ve qué funciona realmente en tu hardware:

```console
sudo modprobe facer enable_all=1
# persistente entre reinicios:
echo "options facer enable_all=1" | sudo tee /etc/modprobe.d/facer-options.conf
```

Esto es solo WMI (sin escrituras EC crudas), así que en hardware que no implementa una función dada es un no-op seguro, no una escritura peligrosa. Por favor [abre un issue](https://github.com/cleyton1986/predator-sense/issues) con tu modelo y lo que funcionó/no funcionó — así es como se añaden nuevos quirks.
</details>

---

## Estructura del Proyecto

```
predator-sense-gui/
├── kernel/                      # Módulos del kernel Linux (gestionados por DKMS)
│   ├── facer.c                  # Interfaz ACPI/WMI hacia el hardware Acer
│   ├── acer-wmi-battery.c       # Soporte de límite de carga de batería
│   ├── acpi_ec.c                # Acceso EC crudo vía /dev/ec (de MusiKid/acpi_ec)
│   ├── Makefile
│   └── dkms.conf                # Configuración de recompilación automática de DKMS
├── installer/                   # Instalador multicall y servicios en Rust
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # Despacho tipado según el nombre del ejecutable instalado
│       ├── constants.rs         # Rutas centrales, valores de protocolo y constantes de hardware
│       ├── install.rs           # Instalador + registro DKMS
│       ├── helper.rs            # Operaciones de hardware privilegiadas validadas
│       ├── hotkey.rs            # Listener de eventos de entrada de Linux
│       ├── tray.rs              # Servicio StatusNotifierItem
│       └── i18n.rs              # Mensajes tipados EN/PT
├── protocol/                    # Contrato tipado compartido entre GUI y helper
│   ├── Cargo.toml
│   └── src/lib.rs               # Acciones, rutas, límites y nombres de binarios
├── src/                         # Aplicación Rust GTK4
│   ├── main.rs
│   ├── app_state.rs             # Flag global de visibilidad de ventana (bloquea temporizadores)
│   ├── i18n.rs                  # Internacionalización EN/PT
│   ├── config.rs                # Preferencias de usuario (JSON)
│   ├── tray.rs                  # Ciclo de vida del servicio de bandeja en Rust
│   ├── hardware/
│   │   ├── helper.rs            # Cliente tipado del helper privilegiado
│   │   ├── rgb.rs               # RGB vía /dev/acer-gkbbl-*
│   │   ├── hwmon.rs             # Índice de /sys/class/hwmon (caché en OnceLock)
│   │   ├── sensors.rs           # Temperaturas, ventiladores, RAM, red
│   │   ├── gpu.rs               # Parser de nvidia-smi con caché TTL
│   │   ├── procs.rs             # Muestreador de /proc (CPU por núcleo, memoria, lista de procesos)
│   │   ├── storage.rs           # Uso de disco vía df
│   │   ├── sysinfo.rs           # Especificaciones de DMI + CPU + GPU + SO
│   │   ├── fan.rs               # Modo de ventilador + CoolBoost
│   │   ├── extras.rs            # Límite de batería, LCD overdrive, carga USB, animación de arranque
│   │   ├── profile.rs           # CPU governor + EPP + potencia de GPU
│   │   ├── ai_assistant.rs      # Tool-calling de Ollama: lista de permitidos fija mapeada a los setters de hardware:: ya existentes
│   │   ├── ai_snapshot.rs       # Instantánea efímera del estado del hardware, entregada a la IA y borrada tras cada lectura
│   │   ├── ai_actionlog.rs      # Log persistente y revisable de todo lo que la IA sugirió/aplicó
│   │   └── setup.rs             # Gestión del módulo del kernel
│   └── ui/                      # Páginas GTK4 (widgets personalizados con Cairo)
│       ├── window.rs            # Ventana principal, barra lateral, barras neón, ocultar a la bandeja
│       ├── dashboard_page.rs    # Hero + especificaciones del sistema
│       ├── temperatures_page.rs # Todos los medidores de temperatura
│       ├── usage_page.rs        # CPU/GPU/Mem/Almacenamiento con los procesos principales
│       ├── network_page.rs      # Descarga/subida con seguimiento de picos
│       ├── rgb_page.rs          # RGB del teclado con zonas visuales
│       ├── fan_control_page.rs  # Ventiladores animados + CoolBoost
│       ├── fan_page.rs          # Perfiles de rendimiento
│       ├── battery_page.rs      # Estadísticas de batería + límite de carga
│       ├── gpu_page.rs          # Panel NVIDIA GPU
│       ├── monitor_page.rs      # Gráficos históricos detallados de CPU/GPU
│       ├── ai_page.rs           # Asistente de IA: chat, gestor de modelos, monitor de recursos, log de acciones
│       ├── setup_page.rs        # Asistente de configuración del módulo del kernel
│       └── gauge_widget.rs      # Widget de medidor circular discontinuo
└── resources/
    ├── style.css                # Tema oscuro gaming
    └── predator-icon.svg        # Icono de la bandeja del sistema
```

---

## Créditos y Agradecimientos

- **Módulo del kernel `facer`** basado en el proyecto [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) de [JafarAkhondali](https://github.com/JafarAkhondali) y [todos los colaboradores](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module/graphs/contributors)
- **Módulo del kernel `acpi_ec`** por [Sayafdine Said (MusiKid)](https://github.com/MusiKid/acpi_ec) — expone `/dev/ec` para lectura/escritura EC cruda. Usado por el helper para establecer modos de ventilador, CoolBoost, LCD overdrive, carga USB y animación de arranque.
- **Aplicación GUI** construida con [Rust](https://www.rust-lang.org/) + [GTK4](https://gtk.org/) + [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/)
- **Instalador y servicios en segundo plano** construidos con [Rust](https://www.rust-lang.org/); la integración con la bandeja usa [ksni](https://crates.io/crates/ksni)
- **Iconos de Dashboard y Temperaturas** (`predator-sense-gui/resources/icons/`) de [Flaticon](https://www.flaticon.com), creados por Hilmy Abiyyu A., magnific y mehwish

### Hacer un Fork o Reutilizar este Proyecto

Este proyecto está licenciado bajo GPL-3.0, así que eres libre de hacer fork, modificarlo y redistribuirlo bajo la misma licencia. Si lo haces — especialmente si construyes una app derivada o reutilizas partes significativas de la GUI/módulo del kernel — **por favor mantén un crédito visible al autor original** (basta con mencionar a [Cleyton Alves](https://github.com/cleyton1986) / este repositorio en tu README, pantalla de Acerca de, o sección de créditos). Es una pequeña petición que significa mucho para un proyecto paralelo independiente y no remunerado.

## Apoya el Proyecto

Si este proyecto te fue útil y quieres apoyar su desarrollo, considera invitarme a un café:

<p align="center">
  <a href="https://www.paypal.com/cgi-bin/webscr?cmd=_donations&business=cleyton1986%40gmail.com&currency_code=BRL&item_name=Predator+Sense+for+Linux">
    <img src="https://img.shields.io/badge/PayPal-Donate-00457C?logo=paypal&logoColor=white&style=for-the-badge" alt="Donate via PayPal">
  </a>
</p>

<p align="center">
  <b>PIX (Brasil):</b> <code>cleyton1986@gmail.com</code>
</p>

¡Cualquier contribución es voluntaria y muy apreciada! Ayuda a mantener vivo el proyecto y motiva nuevas funciones.

---

## Licencia

Este proyecto está licenciado bajo la **GNU General Public License v3.0** — ver el archivo [LICENSE](LICENSE) para más detalles.

Este es software libre: puedes redistribuirlo y/o modificarlo bajo los términos de la GNU GPL publicada por la Free Software Foundation.

**Excepción — imágenes de productos:** la licencia GPLv3 anterior cubre únicamente el código fuente de este proyecto. Las fotos de portátiles Acer Predator/Nitro en `predator-sense-gui/resources/models/` son imágenes de productos de terceros (ver [Descargo de responsabilidad](#descargo-de-responsabilidad) arriba) y **no** están cubiertas por la concesión de GPLv3; todos los derechos sobre esas imágenes permanecen con Acer Inc. y/o los fotógrafos originales.

**Este software se proporciona "tal cual", sin garantía de ningún tipo.** Los autores no se hacen responsables de ningún daño que pueda surgir del uso de este software. Al instalar y usar este software, reconoces que lo haces bajo tu propia responsabilidad.
