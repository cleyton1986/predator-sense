// The reference design's hardware rules (its page, sha256
// d657749ae70bc229a3562c4ff5c5b0ef21cb3042152c19e40cadfb53e4056981),
// cut out of its page script unchanged:
//   TH, BUILTIN, TOP, MORE, STR, T,
//   MODELS, LAYOUTS, PALETTE7, nearest7, hexRgb, FW_KIND,
//   KIND_NAMES, SIM, withLook, fwNearest, themeLights, PER_KEY_PATTERNS,
//   BAR_THEMES, themeAvail, HIDE_WHY, renderGalleries.
// derive.js evaluates them with effects.js as F.
const KB = F.PH16_71;
const TH = {};
  F.THEMES.forEach(function (t) { TH[t.id] = t; });
  const BUILTIN = F.THEMES.slice().sort(function (a, b) { return (a.top || 99) - (b.top || 99); });
  const TOP = BUILTIN.filter(function (t) { return t.top; }), MORE = BUILTIN.filter(function (t) { return !t.top; });
const STR = {
    en: { lighting: 'Lighting', lightingFor: 'Lighting for', active: 'Active', isActive: '{0} is active.', quiet: 'Quiet', eco: 'Eco', balanced: 'Balanced', performance: 'Performance', turbo: 'Turbo',
      preset: 'Preset', noPreset: 'No preset', noPresets: 'No presets yet. Save this mode\'s lighting to use it again or on another mode.', changed: 'Changed', changedLow: 'changed',
      discard: 'Discard changes', save: 'Save', saveMenu: 'More save options', saveAsNew: 'Save as new preset...', saveAs: 'Save as preset...', load: 'Load preset', more: 'More actions',
      rename: 'Rename...', del: 'Delete', name: 'Name', cancel: 'Cancel', nameEmpty: 'Give the preset a name.', nameTaken: 'A preset called {0} already exists.', onModes: 'on {0}', unused: 'not on a mode',
      tabControl: 'Lighting control', tabThemes: 'Themes', inUse: 'in use', previewOnly: 'Preview only: the laptop shows {0}.', showsNow: 'The laptop shows this now.', showOnLaptop: 'Show on laptop',
      pause: 'Pause', play: 'Play', keyboard: 'Keyboard', lightBar: 'Light bar', coverLogo: 'Cover logo', light: 'Light', brightness: 'Brightness',
      kbOwn: 'The keyboard\'s own effects', barOwn: 'The light bar\'s own effects', logoOwn: 'The logo\'s own effects', effect: 'Effect', perKey: 'Per-key colours',
      colours: 'Colours', colour: 'Colour', speed: 'Speed', direction: 'Direction', slow: 'Slow', fast: 'Fast', custom: 'Custom', settings: 'Settings', manual: 'Manual', themeOf: '{0} theme', off: 'Off',
      cSettings: 'Colour, speed and direction', usesTheme: '{0} uses the {1} theme. Changing anything here switches it to Manual.', themes: 'Themes', showMore: 'Show {0} more themes', showFewer: 'Show fewer themes',
      idleTitle: 'When idle', idleDesc: 'Lighting turns off after a while without input, in every mode. Clicks always wake it.', idleSwitch: 'Turn lighting off when idle', turnOffAfter: 'Turn off after',
      turnsOffAfter: 'Turns off after {0}', staysOn: 'Stays on while idle', mouse: 'Moving the mouse wakes the lights', mouseShort: 'Clicks always count, with or without this',
      chooseKeys: 'Choose keys', chooseKey: 'Choose the key', chooseList: 'Choose from a list...', done: 'Done', pickHint: 'Click keys or press them. Arrow keys move the cursor, Space chooses. Fn, Power and the Windows key can only be clicked.',
      paintHint: 'Click or drag over keys, or press them. Arrow keys move the cursor, Space paints. Ctrl+Z undoes a stroke.', brush: 'Brush', paintGroup: 'Paint', allKeys: 'All keys', clearAll: 'Clear all', undo: 'Undo',
      resetTheme: 'Reset to built-in', ownEffect: 'The keyboard\'s own effect, no CPU', animated: 'Animated by Predator Sense', reacts: 'Reacts to typing', notOffered: 'Not offered on this laptop',
      pickBelow: 'Pick one of the themes below for {0}.', usesManual: '{0} uses Manual lighting. Picking a theme below replaces it, with Undo.' },
    de: { lighting: 'Beleuchtung', lightingFor: 'Beleuchtung für', active: 'Aktiv', isActive: '{0} ist aktiv.', quiet: 'Leise', eco: 'Eco', balanced: 'Ausgeglichen', performance: 'Leistung', turbo: 'Turbo',
      preset: 'Voreinstellung', noPreset: 'Keine Voreinstellung', noPresets: 'Noch keine Voreinstellungen. Speichere die Beleuchtung dieses Modus, um sie wieder oder in einem anderen Modus zu nutzen.', changed: 'Geändert', changedLow: 'geändert',
      discard: 'Änderungen verwerfen', save: 'Speichern', saveMenu: 'Weitere Speicheroptionen', saveAsNew: 'Als neue Voreinstellung speichern...', saveAs: 'Als Voreinstellung speichern...', load: 'Voreinstellung laden', more: 'Weitere Aktionen',
      rename: 'Umbenennen...', del: 'Löschen', name: 'Name', cancel: 'Abbrechen', nameEmpty: 'Gib der Voreinstellung einen Namen.', nameTaken: 'Eine Voreinstellung namens {0} gibt es schon.', onModes: 'in {0}', unused: 'in keinem Modus',
      tabControl: 'Beleuchtungssteuerung', tabThemes: 'Designs', inUse: 'in Verwendung', previewOnly: 'Nur Vorschau: Der Laptop zeigt {0}.', showsNow: 'Der Laptop zeigt das gerade.', showOnLaptop: 'Auf dem Laptop zeigen',
      pause: 'Pause', play: 'Abspielen', keyboard: 'Tastatur', lightBar: 'Lichtleiste', coverLogo: 'Deckel-Logo', light: 'Licht', brightness: 'Helligkeit',
      kbOwn: 'Eigene Effekte der Tastatur', barOwn: 'Eigene Effekte der Lichtleiste', logoOwn: 'Eigene Effekte des Logos', effect: 'Effekt', perKey: 'Farben pro Taste',
      colours: 'Farben', colour: 'Farbe', speed: 'Geschwindigkeit', direction: 'Richtung', slow: 'Langsam', fast: 'Schnell', custom: 'Eigene', settings: 'Einstellungen', manual: 'Manuell', themeOf: 'Design {0}', off: 'Aus',
      cSettings: 'Farbe, Geschwindigkeit und Richtung', usesTheme: '{0} nutzt das Design {1}. Jede Änderung hier stellt auf Manuell um.', themes: 'Designs', showMore: '{0} weitere Designs zeigen', showFewer: 'Weniger Designs zeigen',
      idleTitle: 'Bei Inaktivität', idleDesc: 'Die Beleuchtung geht nach einer Weile ohne Eingabe aus, in jedem Modus. Klicks wecken sie immer.', idleSwitch: 'Beleuchtung bei Inaktivität ausschalten', turnOffAfter: 'Ausschalten nach',
      turnsOffAfter: 'Geht nach {0} aus', staysOn: 'Bleibt bei Inaktivität an', mouse: 'Mausbewegungen wecken die Beleuchtung', mouseShort: 'Klicks zählen immer, mit oder ohne diese Option',
      chooseKeys: 'Tasten wählen', chooseKey: 'Taste wählen', chooseList: 'Aus einer Liste wählen...', done: 'Fertig', pickHint: 'Tasten anklicken oder drücken. Pfeiltasten bewegen den Cursor, Leertaste wählt. Fn, Ein/Aus und die Windows-Taste lassen sich nur anklicken.',
      paintHint: 'Über Tasten klicken oder ziehen, oder sie drücken. Pfeiltasten bewegen den Cursor, Leertaste malt. Strg+Z macht einen Strich rückgängig.', brush: 'Pinsel', paintGroup: 'Malen', allKeys: 'Alle Tasten', clearAll: 'Alles löschen', undo: 'Rückgängig',
      resetTheme: 'Auf Standard zurücksetzen', ownEffect: 'Eigener Effekt der Tastatur, keine CPU', animated: 'Von Predator Sense animiert', reacts: 'Reagiert aufs Tippen', notOffered: 'Auf diesem Laptop nicht angeboten',
      pickBelow: 'Wähle unten ein Design für {0}.', usesManual: '{0} nutzt manuelle Beleuchtung. Ein Design unten ersetzt sie, mit Rückgängig.' },
    pt: { lighting: 'Iluminação', lightingFor: 'Iluminação para', active: 'Ativo', isActive: '{0} está ativo.', quiet: 'Silencioso', eco: 'Eco', balanced: 'Balanceado', performance: 'Performance', turbo: 'Turbo',
      preset: 'Predefinição', noPreset: 'Nenhuma predefinição', noPresets: 'Ainda não há predefinições. Salve a iluminação deste modo para usá-la de novo ou em outro modo.', changed: 'Alterado', changedLow: 'alterado',
      discard: 'Descartar alterações', save: 'Salvar', saveMenu: 'Mais opções de salvar', saveAsNew: 'Salvar como nova predefinição...', saveAs: 'Salvar como predefinição...', load: 'Carregar predefinição', more: 'Mais ações',
      rename: 'Renomear...', del: 'Excluir', name: 'Nome', cancel: 'Cancelar', nameEmpty: 'Dê um nome à predefinição.', nameTaken: 'Já existe uma predefinição chamada {0}.', onModes: 'em {0}', unused: 'em nenhum modo',
      tabControl: 'Controle de iluminação', tabThemes: 'Temas', inUse: 'em uso', previewOnly: 'Só pré-visualização: o notebook mostra {0}.', showsNow: 'O notebook mostra isto agora.', showOnLaptop: 'Mostrar no notebook',
      pause: 'Pausar', play: 'Reproduzir', keyboard: 'Teclado', lightBar: 'Barra de luz', coverLogo: 'Logotipo da tampa', light: 'Luz', brightness: 'Brilho',
      kbOwn: 'Efeitos próprios do teclado', barOwn: 'Efeitos próprios da barra de luz', logoOwn: 'Efeitos próprios do logotipo', effect: 'Efeito', perKey: 'Cores por tecla',
      colours: 'Cores', colour: 'Cor', speed: 'Velocidade', direction: 'Direção', slow: 'Lento', fast: 'Rápido', custom: 'Personalizado', settings: 'Configurações', manual: 'Manual', themeOf: 'Tema {0}', off: 'Desligado',
      cSettings: 'Cor, velocidade e direção', usesTheme: '{0} usa o tema {1}. Qualquer mudança aqui passa para Manual.', themes: 'Temas', showMore: 'Mostrar mais {0} temas', showFewer: 'Mostrar menos temas',
      idleTitle: 'Quando ocioso', idleDesc: 'A iluminação desliga após um tempo sem entrada, em todos os modos. Cliques sempre a ativam.', idleSwitch: 'Desligar a iluminação quando ocioso', turnOffAfter: 'Desligar após',
      turnsOffAfter: 'Desliga após {0}', staysOn: 'Não desliga por inatividade', mouse: 'Mover o mouse acorda a iluminação', mouseShort: 'Cliques sempre contam, com ou sem isto',
      chooseKeys: 'Escolher teclas', chooseKey: 'Escolher a tecla', chooseList: 'Escolher de uma lista...', done: 'Concluir', pickHint: 'Clique nas teclas ou pressione-as. As setas movem o cursor, Espaço escolhe. Fn, Liga/Desliga e a tecla Windows só podem ser clicadas.',
      paintHint: 'Clique ou arraste sobre as teclas, ou pressione-as. As setas movem o cursor, Espaço pinta. Ctrl+Z desfaz um traço.', brush: 'Pincel', paintGroup: 'Pintar', allKeys: 'Todas as teclas', clearAll: 'Limpar tudo', undo: 'Desfazer',
      resetTheme: 'Restaurar o original', ownEffect: 'Efeito próprio do teclado, sem CPU', animated: 'Animado pelo Predator Sense', reacts: 'Reage à digitação', notOffered: 'Não oferecido neste notebook',
      pickBelow: 'Escolha um dos temas abaixo para {0}.', usesManual: '{0} usa iluminação manual. Escolher um tema abaixo a substitui, com Desfazer.' },
    ru: { lighting: 'Подсветка', lightingFor: 'Подсветка для', active: 'Активен', isActive: 'Режим «{0}» активен.', quiet: 'Тихий', eco: 'Eco', balanced: 'Сбалансированный', performance: 'Производительность', turbo: 'Турбо',
      preset: 'Пресет', noPreset: 'Нет пресета', noPresets: 'Пресетов пока нет. Сохраните подсветку этого режима, чтобы вернуться к ней или использовать в другом режиме.', changed: 'Изменено', changedLow: 'изменено',
      discard: 'Отменить изменения', save: 'Сохранить', saveMenu: 'Другие варианты сохранения', saveAsNew: 'Сохранить как новый пресет...', saveAs: 'Сохранить как пресет...', load: 'Загрузить пресет', more: 'Другие действия',
      rename: 'Переименовать...', del: 'Удалить', name: 'Название', cancel: 'Отмена', nameEmpty: 'Дайте пресету название.', nameTaken: 'Пресет «{0}» уже существует.', onModes: 'в режимах: {0}', unused: 'ни в одном режиме',
      tabControl: 'Управление подсветкой', tabThemes: 'Темы', inUse: 'используется', previewOnly: 'Только предпросмотр: на ноутбуке режим «{0}».', showsNow: 'Сейчас это видно на ноутбуке.', showOnLaptop: 'Показать на ноутбуке',
      pause: 'Пауза', play: 'Пуск', keyboard: 'Клавиатура', lightBar: 'Световая полоса', coverLogo: 'Логотип на крышке', light: 'Источник', brightness: 'Яркость',
      kbOwn: 'Собственные эффекты клавиатуры', barOwn: 'Собственные эффекты световой полосы', logoOwn: 'Собственные эффекты логотипа', effect: 'Эффект', perKey: 'Цвета клавиш',
      colours: 'Цвета', colour: 'Цвет', speed: 'Скорость', direction: 'Направление', slow: 'Медленно', fast: 'Быстро', custom: 'Свои', settings: 'Настройки', manual: 'Вручную', themeOf: 'Тема «{0}»', off: 'Выкл.',
      cSettings: 'Цвет, скорость и направление', usesTheme: 'Режим «{0}» использует тему «{1}». Любое изменение здесь переключит его на ручную настройку.', themes: 'Темы', showMore: 'Показать ещё {0} тем', showFewer: 'Показать меньше тем',
      idleTitle: 'В простое', idleDesc: 'Подсветка выключается после периода без ввода, в любом режиме. Щелчок всегда её включает.', idleSwitch: 'Выключать подсветку в простое', turnOffAfter: 'Выключать через',
      turnsOffAfter: 'Выключается через {0}', staysOn: 'Не выключается в простое', mouse: 'Движение мыши включает подсветку', mouseShort: 'Щелчки учитываются всегда',
      chooseKeys: 'Выбрать клавиши', chooseKey: 'Выбрать клавишу', chooseList: 'Выбрать из списка...', done: 'Готово', pickHint: 'Щёлкайте по клавишам или нажимайте их. Стрелки двигают курсор, пробел выбирает. Fn, питание и клавишу Windows можно только щёлкнуть.',
      paintHint: 'Щёлкайте или ведите по клавишам, или нажимайте их. Стрелки двигают курсор, пробел красит. Ctrl+Z отменяет мазок.', brush: 'Кисть', paintGroup: 'Покрасить', allKeys: 'Все клавиши', clearAll: 'Очистить всё', undo: 'Отменить',
      resetTheme: 'Вернуть исходную', ownEffect: 'Собственный эффект клавиатуры, без нагрузки на CPU', animated: 'Анимирует Predator Sense', reacts: 'Реагирует на набор', notOffered: 'На этом ноутбуке недоступно',
      pickBelow: 'Выберите тему ниже для режима «{0}».', usesManual: 'Режим «{0}» использует ручную подсветку. Тема ниже заменит её, с возможностью отмены.' },
  };
function T(key) {
    const lang = (typeof S !== 'undefined' && S && S.prefs && STR[S.prefs.lang]) ? S.prefs.lang : 'en';
    let s = STR[lang][key] !== undefined ? STR[lang][key] : STR.en[key] !== undefined ? STR.en[key] : key;
    for (let i = 1; i < arguments.length; i++) s = s.split('{' + (i - 1) + '}').join(arguments[i]);
    return s;
  }
const MODELS = {
    'PH16-71': { label: 'PH16-71 (this laptop)', side: 'Predator PH16-71', kb: 'perkey', kbKind: 'chicony', bar: 'zones3', logo: null, live: true,
      found: 'Keyboard (103 keys, each its own colour) and light bar (3 zones).', note: 'This laptop: everything as designed, with its real key layout.' },
    'PH16-72': { label: 'PH16-72 / PH18-72', side: 'Predator PH16-72', kb: 'one', kbKind: 'sunrex', bar: 'one', logo: 'darfon', reported: true,
      found: 'Keyboard (one colour), light bar (one colour) and cover logo; keyboard and logo reported by users, not confirmed.',
      note: 'Sunrex keyboard (16 effects and Off), light bar with 14 effects and Off, Darfon cover logo. The keyboard and logo are reported, not confirmed.' },
    'PHN16-73': { label: 'PHN16-73', side: 'Predator PHN16-73', kb: 'zones4', kbKind: 'ene', eneOffered: 'Only if the keyboard offers it', bar: null, logo: 'ene',
      found: 'Keyboard (4 zones) and cover logo.', note: 'ENE keyboard, 4 zones, and its cover logo. No light bar.' },
    'PHN16S-71': { label: 'PHN16S-71 / AN16S-61', side: 'Predator PHN16S-71', kb: 'zones4', kbKind: 'ene', eneOffered: 'Always on the PHN16S-71; on the AN16S-61 only if the keyboard offers it', bar: null, logo: null,
      found: 'Keyboard (4 zones).', note: 'ENE keyboard, 4 zones, nothing else.' },
    'WMI': { label: 'Older 4-zone models', side: 'Older 4-zone model', kb: 'zones4', kbKind: 'wmi', bar: null, logo: null,
      found: 'Keyboard (4 zones).', note: 'Older WMI keyboards (Nitro AN515 and AN517, Predator PH315, PH317-53/54/55, the PT314 to PT917 family): 4 still zones and 7 moving effects.' },
    'PH317-56': { label: 'Helios 300 (PH317-56)', side: 'Predator PH317-56', kb: 'palette', kbKind: 'palette', bar: null, logo: null,
      found: 'Keyboard (one colour at a time, from 7 fixed colours).', note: 'Chicony palette keyboard: 12 effects and 7 fixed colours, no colour of your own.' },
    'PT14-51': { label: 'PT14-51', side: 'Predator PT14-51', kb: 'one', kbKind: 'sunrex', direct: true, bar: null, logo: null,
      found: 'Keyboard (one colour).', note: 'Sunrex keyboard. Its per-key mode (Direct) is experimental and not confirmed on hardware.' },
    'NONE': { label: 'No lighting', side: 'Acer laptop', kb: null, bar: null, logo: null,
      found: 'No lighting that Predator Sense can control was found on this laptop.', note: 'A laptop with no lighting the app can drive.' },
  };
const LAYOUTS = {
    perkey: KB, zones4: F.zoneLayout(4, 'keyboard'), kbone: F.zoneLayout(1, 'keyboard'),
    zones3: F.zoneLayout(3, 'bar'), barone: F.zoneLayout(1, 'bar'), logo: F.zoneLayout(1, 'logo'),
  };
const PALETTE7 = [['Red', '#FF0000'], ['Green', '#00FF00'], ['Yellow', '#FFFF00'], ['Dark Blue', '#0000FF'], ['Bright Blue', '#00BFFF'], ['Bright Pink', '#FF3CC8'], ['White', '#FFFFFF']];
function nearest7(hex) {
    const c = hexRgb(hex);
    let best = PALETTE7[0][1], bd = 1e9;
    PALETTE7.forEach(function (p) { const q = hexRgb(p[1]), d = (c[0] - q[0]) * (c[0] - q[0]) * 0.3 + (c[1] - q[1]) * (c[1] - q[1]) * 0.59 + (c[2] - q[2]) * (c[2] - q[2]) * 0.11; if (d < bd) { bd = d; best = p[1]; } });
    return best;
  }
function hexRgb(h) { const v = parseInt(h.slice(1), 16); return [(v >> 16) & 255, (v >> 8) & 255, v & 255]; }
const FW_KIND = { static: 'still', spot: 'still', ripple: 'still', fireball: 'still', snow: 'still', heartbeat: 'still',
    breathing: 'pulse', stars: 'pulse', rain: 'pulse', snake: 'pulse', lightning: 'pulse', wave: 'cycle', rainbow: 'cycle' };
const KIND_NAMES = {   // the names of each keyboard's own effects for those three kinds
    ene: { still: 'Static', pulse: 'Breath', cycle: 'Neon' }, wmi: { still: 'Static', pulse: 'Breath', cycle: 'Neon' },
    sunrex: { still: 'Static', pulse: 'Breathing', cycle: 'Neon' }, palette: { still: 'Static', pulse: 'Static', cycle: 'Static' } };
const SIM = { still: 'hw.static', pulse: 'hw.breathing', cycle: 'hw.neon' };
function withLook(th, which, look) { const o = Object.assign({}, th); o[which] = look; return o; }
function fwNearest(M, look) { return (FW_KIND[look.hw] || 'still'); }
function themeLights(th, M) {
    const L = {};
    const lay = M.kb === 'perkey' ? LAYOUTS.perkey : M.kb === 'zones4' ? LAYOUTS.zones4 : LAYOUTS.kbone;
    if (M.kb === 'perkey') L.kb = F.lookFor(th, 'keyboard', LAYOUTS.perkey);
    else if (M.kb === 'palette') {
      const look = th.keyboard, col = look.hw ? th.palette[(look.colors || [0])[0] % th.palette.length] : th.palette[0];
      L.kb = F.createEffect('hw.static', {}, [nearest7(col)], LAYOUTS.kbone, { role: 'keyboard' });
    } else if (M.kb) {
      const look = th.keyboard;
      if (look.hw) {
        const kind = fwNearest(M, look), col = th.palette[(look.colors || [0])[0] % th.palette.length];
        L.kb = F.createEffect(SIM[kind], { speed: 1 }, [col], lay, { role: 'keyboard' });
      } else L.kb = F.lookFor(th, 'keyboard', lay);
    }
    if (M.bar === 'zones3') L.bar = F.lookFor(th, 'bar', LAYOUTS.zones3);
    else if (M.bar === 'one') {
      let look = th.bar && th.bar.hw ? th.bar : th.barFallback || { hw: 'breathing', colors: [0], speed: 1 };
      const cs = look.colors || [0];
      look = Object.assign({}, look, { colors: [cs.length > 1 ? cs[1] : cs[0]] });
      L.bar = F.lookFor(withLook(th, 'bar', look), 'bar', LAYOUTS.barone);
    }
    if (M.logo && th.logo) {
      let look = th.logo;
      if (look.hw === 'neon' && M.logo === 'darfon') look = { hw: 'breathing', colors: look.colors, speed: look.speed };
      L.logo = F.lookFor(withLook(th, 'logo', look), 'logo', LAYOUTS.logo);
    }
    return L;
  }
const PER_KEY_PATTERNS = ['matrix', 'sakura', 'snowfall', 'grid', 'starlight', 'neon-rain', 'comet', 'pond', 'frost', 'heartline',
    'serpent', 'predator-classic', 'neon-tokyo', 'vision', 'storm'];
const BAR_THEMES = ['kitt'];
function themeAvail(t, M) {
    if (!M.kb && !M.bar && !M.logo) return 'nolights';
    if (M.kb === 'perkey') return M.bar || BAR_THEMES.indexOf(t.id) < 0 ? null : 'bar';
    if ((t.settings || []).some(function (s) { return s.type === 'keys' || s.type === 'key'; })) return 'keys';
    if (M.kb === 'palette') return t.keyboard.hw === 'static' ? null : 'colours';
    if (PER_KEY_PATTERNS.indexOf(t.id) >= 0) return 'perkey';
    if (BAR_THEMES.indexOf(t.id) >= 0 && !M.bar) return 'bar';
    return null;
  }
const HIDE_WHY = { keys: 'need key choices (per-key keyboards only)', perkey: 'are patterns on single keys that this keyboard cannot show',
    bar: 'need a light bar', colours: 'need colours or effects this keyboard does not have (it shows 7 fixed colours)' };
function renderGalleries() {
    cards.forEach(function (c) { if (io) io.unobserve(c.cv); });
    cards = [];
    $('galTitle').textContent = T('themes');
    const M = MODELS[S.model], ok = function (t) { return !themeAvail(t, M); };
    const top = $('galTop'); top.innerHTML = '';
    const tops = TOP.filter(ok), mores = MORE.filter(ok), shownTop = tops.length ? tops : mores.slice(0, 12), rest = tops.length ? mores : mores.slice(12);
    shownTop.forEach(function (t) { top.append(makeCard(t.id)); });
    const more = $('galMore'); more.innerHTML = '';
    if (S.moreOpen) rest.forEach(function (t) { more.append(makeCard(t.id)); });
    more.hidden = !S.moreOpen || !rest.length;
    const mb = $('moreBtn');
    mb.hidden = !rest.length;
    mb.textContent = S.moreOpen ? T('showFewer') : T('showMore', rest.length);
    mb.setAttribute('aria-expanded', String(S.moreOpen));
    const hidden = {};
    BUILTIN.forEach(function (t) { const w = themeAvail(t, M); if (w) hidden[w] = (hidden[w] || 0) + 1; });
    const keys = Object.keys(hidden), n = keys.reduce(function (a, k) { return a + hidden[k]; }, 0), note = $('hiddenNote');
    note.innerHTML = '';
    if (n) {
      const why = keys.indexOf('colours') >= 0 ? 'need colours this keyboard cannot show' : keys.indexOf('bar') >= 0 ? 'need per-key lighting or a light bar' : 'need per-key lighting';
      const detail = keys.map(function (k) { return hidden[k] + ' ' + HIDE_WHY[k]; }).join('; ');
      note.append(document.createTextNode(n + ' themes ' + why + ' and are hidden here.'), h('span', { class: 'sr', text: ' ' + detail + '.' }));
      note.title = detail;
    }
  }
