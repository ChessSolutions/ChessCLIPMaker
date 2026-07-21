const state = {
  parsedGame: null,
  timeline: [],
  selectedRange: [0, 0],
  importedUsername: null,
  savedPlayers: [],
};

const pgnInput = document.getElementById('pgn');
const lichessInput = document.getElementById('lichess-url');
const gameHistoryOffsetInput = document.getElementById('game-history-offset');
const usernameSiteInput = document.getElementById('username-site');
const playerSuggestions = document.getElementById('saved-player-suggestions');
const moveStartInput = document.getElementById('move-start');
const moveEndInput = document.getElementById('move-end');
const moveDelayInput = document.getElementById('move-delay');
const captionInput = document.getElementById('caption-text');
const captionFontInput = document.getElementById('caption-font');
const captionFontSizeInput = document.getElementById('caption-font-size');
const captionAlignInput = document.getElementById('caption-align');
const captionPaddingInput = document.getElementById('caption-padding');
const captionColorInput = document.getElementById('caption-color');
const captionBackgroundInput = document.getElementById('caption-background');
const captionDurationInput = document.getElementById('caption-duration');
const captionLivePreview = document.getElementById('caption-live-preview');
const captionLiveText = document.getElementById('caption-live-text');
const captionLiveSection = document.getElementById('caption-live-section');
const captionCharacterCount = document.getElementById('caption-character-count');
const captionEditorPanel = document.getElementById('caption-editor-panel');
const captionEditorToggle = document.getElementById('toggle-caption-editor');
const moveRangePanel = document.getElementById('move-range-panel');
const moveRangeToggle = document.getElementById('toggle-move-range');
const mediaPanel = document.getElementById('media-panel');
const mediaPanelToggle = document.getElementById('toggle-media-panel');
const autoTitleCardInput = document.getElementById('auto-title-card');
const captionSettings = document.getElementById('caption-settings');
const mediaFileInput = document.getElementById('media-file');
const mediaDurationInput = document.getElementById('media-duration');
const mediaEditor = document.getElementById('media-editor');
const mediaEditorImage = document.getElementById('media-editor-image');
const mediaScaleInput = document.getElementById('media-scale');
const mediaOffsetXInput = document.getElementById('media-offset-x');
const mediaOffsetYInput = document.getElementById('media-offset-y');
const accountSettings = document.getElementById('account-settings');
const lichessUsernamesInput = document.getElementById('lichess-usernames');
const chesscomUsernamesInput = document.getElementById('chesscom-usernames');
const defaultLichessInput = document.getElementById('default-lichess');
const defaultChesscomInput = document.getElementById('default-chesscom');
const showPlayerBarsInput = document.getElementById('show-player-bars');
const showClocksInput = document.getElementById('show-clocks');
const darkSquareColorInput = document.getElementById('dark-square-color');
const showCoordinatesInput = document.getElementById('show-coordinates');
const showMoveHighlightsInput = document.getElementById('show-move-highlights');
let pendingMedia = null;
let editingCaptionIndex = null;
const statusNode = document.getElementById('status');
const timelineList = document.getElementById('timeline-list');
const previewImage = document.getElementById('preview-image');
const previewStage = document.getElementById('preview-stage');
const previewDimensions = document.getElementById('preview-dimensions');
const previewButton = document.getElementById('preview-gif');
const downloadButton = document.getElementById('download-gif');
const orientationSwitch = document.querySelector('.orientation-options');
let previewInProgress = false;
let downloadInProgress = false;

function updatePreviewDimensions() {
  const withBars = showPlayerBarsInput.checked;
  previewStage.classList.toggle('with-player-bars', withBars);
  previewDimensions.textContent = withBars ? 'GIF · 720 × 840' : 'GIF · 720 × 720';
}

function toggleOrientation() {
  const current = getExportOrientation();
  const next = document.querySelector(`input[name="export-orientation"][value="${current === 'white' ? 'black' : 'white'}"]`);
  if (next) next.checked = true;
  orientationSwitch.setAttribute('aria-checked', String(getExportOrientation() === 'black'));
}

orientationSwitch.addEventListener('click', (event) => {
  event.preventDefault();
  toggleOrientation();
});
orientationSwitch.addEventListener('keydown', (event) => {
  if (event.key === ' ' || event.key === 'Enter') {
    event.preventDefault();
    toggleOrientation();
  }
});

function setStatus(message, isError = false) {
  statusNode.textContent = message;
  statusNode.style.color = isError ? '#fb7185' : '#94a3b8';
}

function renderTimeline() {
  timelineList.innerHTML = '';
  if (!state.timeline.length) {
    const empty = document.createElement('li');
    empty.textContent = 'No frames yet';
    timelineList.appendChild(empty);
    return;
  }

  state.timeline.forEach((frame, index) => {
    const item = document.createElement('li');
    item.draggable = true;
    item.dataset.index = String(index);
    const label = document.createElement('span');
    label.className = 'timeline-label';
    label.textContent = frame.type === 'caption' ? `Caption: ${frame.text}` : frame.type === 'media' ? `Media: ${frame.name || 'image'}` : frame.move_label || 'Chess position';
    const order = document.createElement('input');
    order.type = 'number';
    order.min = '1';
    order.value = String(frame.sort_order || index + 1);
    order.className = 'frame-order';
    order.setAttribute('aria-label', `Frame number for ${label.textContent}`);
    order.title = 'Frame number used when sorting';
    order.addEventListener('pointerdown', (event) => event.stopPropagation());
    order.addEventListener('dragstart', (event) => event.stopPropagation());
    order.addEventListener('change', () => {
      const destination = Math.min(state.timeline.length, Math.max(1, Number(order.value) || index + 1));
      if (destination - 1 === index) {
        order.value = String(index + 1);
      } else {
        moveTimelineFrame(index, destination - 1);
      }
    });
    const duration = document.createElement('input');
    duration.type = 'number';
    duration.min = '20';
    duration.max = '30000';
    duration.step = '10';
    duration.value = String(frame.duration_ms);
    duration.className = 'frame-duration';
    duration.title = 'Frame duration in milliseconds';
    duration.addEventListener('change', () => {
      frame.duration_ms = Math.min(30000, Math.max(20, Number(duration.value) || 20));
      duration.value = String(frame.duration_ms);
    });
    const up = document.createElement('button');
    up.className = 'secondary icon-button';
    up.textContent = '↑';
    up.disabled = index === 0;
    up.addEventListener('click', () => moveTimelineFrame(index, index - 1));
    const down = document.createElement('button');
    down.className = 'secondary icon-button';
    down.textContent = '↓';
    down.disabled = index === state.timeline.length - 1;
    down.addEventListener('click', () => moveTimelineFrame(index, index + 1));
    const remove = document.createElement('button');
    remove.className = 'secondary icon-button trash-button';
    remove.textContent = '🗑';
    remove.title = `Remove ${label.textContent}`;
    remove.setAttribute('aria-label', `Remove ${label.textContent}`);
    remove.addEventListener('click', () => {
      state.timeline.splice(index, 1);
      renderTimeline();
    });
    let edit = null;
    if (frame.type === 'caption') {
      edit = document.createElement('button');
      edit.className = 'secondary icon-button';
      edit.textContent = '✎';
      edit.title = `Edit ${label.textContent}`;
      edit.setAttribute('aria-label', `Edit ${label.textContent}`);
      edit.addEventListener('click', () => editCaptionFrame(index));
    }
    item.appendChild(order);
    item.appendChild(label);
    item.appendChild(duration);
    item.appendChild(up);
    item.appendChild(down);
    if (edit) item.appendChild(edit);
    item.appendChild(remove);
    item.addEventListener('dragstart', (event) => event.dataTransfer.setData('text/plain', String(index)));
    item.addEventListener('dragover', (event) => event.preventDefault());
    item.addEventListener('drop', (event) => {
      event.preventDefault();
      moveTimelineFrame(Number(event.dataTransfer.getData('text/plain')), index);
    });
    timelineList.appendChild(item);
  });
}

function moveTimelineFrame(from, to) {
  if (from === to || from < 0 || to < 0 || from >= state.timeline.length || to >= state.timeline.length) return;
  const [frame] = state.timeline.splice(from, 1);
  state.timeline.splice(to, 0, frame);
  state.timeline.forEach((item, index) => { item.sort_order = index + 1; });
  renderTimeline();
}

function sortTimeline() {
  state.timeline.sort((left, right) => (left.sort_order || 0) - (right.sort_order || 0));
  state.timeline.forEach((frame, index) => { frame.sort_order = index + 1; });
  renderTimeline();
  setStatus('Timeline sorted by frame number.');
}

function nextSortOrder() {
  return state.timeline.reduce((maximum, frame) => Math.max(maximum, frame.sort_order || 0), 0) + 1;
}

async function parsePgn() {
  const pgn = pgnInput.value.trim();
  if (!pgn) {
    setStatus('Paste a PGN first.', true);
    return;
  }

  try {
    const response = await fetch('/api/pgn/parse', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ pgn }),
    });
    const payload = await response.json();
    if (!response.ok) {
      throw new Error(payload.error || 'PGN parse failed');
    }
    state.parsedGame = payload;
    selectDefaultPlayerOrientation(payload.metadata || {});
    const totalMoves = payload.moves?.length || 0;
    moveStartInput.value = '1';
    moveEndInput.value = String(Math.max(1, totalMoves));
    state.selectedRange = [1, Math.max(1, totalMoves)];
    if (!totalMoves) {
      state.timeline = [];
      renderTimeline();
      throw new Error('No legal chess moves were found in that PGN.');
    }
    buildTimeline();
    if (autoTitleCardInput.checked) addGameTitleCard();
    setStatus(`Parsed ${totalMoves} moves and added them to the GIF timeline.`);
    await generateGif();
  } catch (error) {
    setStatus(error.message, true);
  }
}

function selectDefaultPlayerOrientation(metadata) {
  const savedPlayers = [defaultLichessInput.value, defaultChesscomInput.value]
    .concat(state.importedUsername || [])
    .map((name) => name.trim().toLowerCase())
    .filter(Boolean);
  const white = String(metadata.White || '').trim().toLowerCase();
  const black = String(metadata.Black || '').trim().toLowerCase();
  let orientation = null;
  if (savedPlayers.includes(white)) orientation = 'white';
  if (savedPlayers.includes(black)) orientation = 'black';
  if (orientation) {
    const radio = document.querySelector(`input[name="export-orientation"][value="${orientation}"]`);
    if (radio) radio.checked = true;
  }
}

async function importLichess() {
  const url = lichessInput.value.trim();
  if (!url) {
    setStatus('Enter a Lichess or Chess.com game URL.', true);
    return;
  }

  try {
    const response = await fetch('/api/lichess/import', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url }),
    });
    const payload = await response.json();
    if (!response.ok) {
      throw new Error(payload.error || 'Game import failed');
    }
    pgnInput.value = payload.pgn;
    lichessInput.value = '';
    state.importedUsername = null;
    setStatus('Game imported.');
    await parsePgn();
  } catch (error) {
    setStatus(error.message, true);
  }
}

async function importAndBuild() {
  const source = lichessInput.value.trim();
  if (source && !source.includes('://') && !source.includes('/')) {
    const normalized = source.toLowerCase();
    const onLichess = usernameList(lichessUsernamesInput).some((name) => name.toLowerCase() === normalized);
    const onChesscom = usernameList(chesscomUsernamesInput).some((name) => name.toLowerCase() === normalized);
    const offset = Number(gameHistoryOffsetInput.value) || 0;
    if (onLichess && onChesscom) {
      await importLatestGame(usernameSiteInput.value, source, offset);
    } else if (onLichess || onChesscom) {
      await importLatestGame(onLichess ? 'lichess' : 'chesscom', source, offset);
    } else if (!await importLatestGame('lichess', source, offset, true)) {
      await importLatestGame('chesscom', source, offset);
    }
  } else if (source) {
    await importLichess();
  } else if (pgnInput.value.trim()) {
    state.importedUsername = null;
    await parsePgn();
  } else {
    setStatus('Paste a PGN or enter a Lichess/Chess.com game URL first.', true);
  }
}

async function loadAccounts() {
  const response = await fetch('/api/accounts');
  const accounts = await response.json();
  lichessUsernamesInput.value = (accounts.lichess || []).join('\n');
  chesscomUsernamesInput.value = (accounts.chesscom || []).join('\n');
  refreshDefaultPlayers(defaultLichessInput, accounts.lichess || [], accounts.default_lichess);
  refreshDefaultPlayers(defaultChesscomInput, accounts.chesscom || [], accounts.default_chesscom);
  state.savedPlayers = [...(accounts.lichess || []).map((name) => [name, 'Lichess']), ...(accounts.chesscom || []).map((name) => [name, 'Chess.com'])];
  renderPlayerSuggestions();
}

function renderPlayerSuggestions() {
  const query = lichessInput.value.trim().toLowerCase();
  const matches = state.savedPlayers
    .filter(([name]) => !query || name.toLowerCase().includes(query))
    .slice(0, 8);
  playerSuggestions.innerHTML = '';
  matches.forEach(([name, site]) => {
    const option = document.createElement('button');
    option.type = 'button';
    option.className = 'player-suggestion';
    option.setAttribute('role', 'option');
    const playerName = document.createElement('span');
    const playerSite = document.createElement('small');
    playerName.textContent = name;
    playerSite.textContent = site;
    option.append(playerName, playerSite);
    option.addEventListener('mousedown', (event) => event.preventDefault());
    option.addEventListener('click', () => {
      lichessInput.value = name;
      playerSuggestions.hidden = true;
      lichessInput.dispatchEvent(new Event('input'));
      lichessInput.focus();
    });
    playerSuggestions.appendChild(option);
  });
  playerSuggestions.hidden = matches.length === 0 || document.activeElement !== lichessInput;
}

function usernameList(input) {
  return [...new Set(input.value.split(/\s+/).map((name) => name.trim()).filter(Boolean))];
}

function refreshDefaultPlayers(select, names, selected) {
  select.innerHTML = '';
  names.forEach((name) => {
    const option = document.createElement('option');
    option.value = name;
    option.textContent = `${name}${name === selected ? ' ⭐ Default' : ''}`;
    option.selected = name === selected;
    select.appendChild(option);
  });
}

async function saveAccounts() {
  const accounts = {
    lichess: usernameList(lichessUsernamesInput),
    chesscom: usernameList(chesscomUsernamesInput),
    default_lichess: defaultLichessInput.value || null,
    default_chesscom: defaultChesscomInput.value || null,
  };
  const response = await fetch('/api/accounts', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(accounts) });
  const payload = await response.json();
  if (!response.ok) { setStatus(payload.error || 'Could not save accounts.', true); return; }
  await loadAccounts();
  setStatus('Account lists saved to accounts.json.');
  accountSettings.close();
}

async function importLatestGame(site, requestedUsername = null, offset = 0, silent = false) {
  const username = requestedUsername || (site === 'lichess' ? defaultLichessInput : defaultChesscomInput).value.trim();
  if (!username) {
    setStatus(`Enter and save your ${site === 'lichess' ? 'Lichess' : 'Chess.com'} username first.`, true);
    return false;
  }
  setStatus(`Getting ${username}'s latest game…`);
  try {
    const response = await fetch('/api/latest-game', {
      method: 'POST',
      cache: 'no-store',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ site, username, offset }),
    });
    const payload = await response.json();
    if (!response.ok) throw new Error(payload.error || 'Latest-game import failed');
    pgnInput.value = payload.pgn;
    state.importedUsername = username;
    if (requestedUsername) await autoSaveImportedUsername(site, requestedUsername);
    accountSettings.close();
    await parsePgn();
    return true;
  } catch (error) {
    if (!silent) setStatus(error.message, true);
    return false;
  }
}

async function autoSaveImportedUsername(site, username) {
  const input = site === 'lichess' ? lichessUsernamesInput : chesscomUsernamesInput;
  if (usernameList(input).some((name) => name.toLowerCase() === username.toLowerCase())) return;
  input.value = [...usernameList(input), username].join('\n');
  const accounts = { lichess: usernameList(lichessUsernamesInput), chesscom: usernameList(chesscomUsernamesInput), default_lichess: defaultLichessInput.value || (site === 'lichess' ? username : null), default_chesscom: defaultChesscomInput.value || (site === 'chesscom' ? username : null) };
  await fetch('/api/accounts', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(accounts) });
  await loadAccounts();
}

function buildTimeline() {
  if (!state.parsedGame?.moves?.length) {
    setStatus('Parse a PGN first.', true);
    return;
  }

  const moveDelay = Number(moveDelayInput.value || 1000);
  const start = Math.max(1, Number(moveStartInput.value || 1));
  const end = Math.max(start, Number(moveEndInput.value || start));
  state.selectedRange = [start, end];

  const selectedMoves = state.parsedGame.moves.slice(start - 1, end);
  if (!selectedMoves.length) {
    setStatus('That move range is outside the parsed game.', true);
    return;
  }
  const frames = [];
  selectedMoves.forEach((move) => {
    frames.push({
      type: 'board',
      duration_ms: moveDelay,
      fen: move.fen_after,
      last_move: showMoveHighlightsInput.checked ? move.uci : null,
      check: move.is_check ? 'yes' : 'no',
      move_label: formatMoveLabel(move),
      sort_order: frames.length + 1,
      clock: showClocksInput.checked ? { white: move.white_clock, black: move.black_clock } : null,
    });
  });

  state.timeline = frames;
  renderTimeline();
  setStatus(`Built a timeline from moves ${start} through ${Math.min(end, state.parsedGame.moves.length)}.`);
}

function formatMoveLabel(move) {
  return move.side === 'white'
    ? `${move.move_number}. ${move.san}`
    : `${move.move_number}... ${move.san}`;
}

function addCaptionFrame() {
  const caption = captionInput.value.trim();
  if (!caption) {
    setStatus('Enter caption text first.', true);
    return;
  }
  const captionFrame = {
    type: 'caption',
    duration_ms: Math.min(30000, Math.max(20, Number(captionDurationInput.value) || 2500)),
    text: caption,
    style: {
      font_family: captionFontInput.value,
      font_size: Math.min(140, Math.max(16, Number(captionFontSizeInput.value) || 56)),
      padding: Math.min(200, Math.max(0, Number(captionPaddingInput.value) || 0)),
      horizontal_align: captionAlignInput.value,
      text_color: captionColorInput.value,
      background_color: captionBackgroundInput.value,
    },
    sort_order: nextSortOrder(),
  };
  if (editingCaptionIndex === null) {
    state.timeline.push(captionFrame);
  } else {
    captionFrame.sort_order = state.timeline[editingCaptionIndex].sort_order;
    state.timeline[editingCaptionIndex] = captionFrame;
    editingCaptionIndex = null;
    document.getElementById('add-caption').textContent = 'Add caption frame';
  }
  renderTimeline();
  captionInput.value = '';
  updateCaptionPreview();
  setCaptionEditorOpen(false);
  setStatus('Caption frame added.');
}

function editCaptionFrame(index) {
  const frame = state.timeline[index];
  if (frame?.type !== 'caption') return;
  editingCaptionIndex = index;
  captionInput.value = frame.text;
  captionFontInput.value = frame.style?.font_family || 'Noto Sans';
  captionFontSizeInput.value = frame.style?.font_size || 56;
  captionAlignInput.value = frame.style?.horizontal_align || 'center';
  captionPaddingInput.value = frame.style?.padding ?? 48;
  captionColorInput.value = frame.style?.text_color || '#ffffff';
  captionBackgroundInput.value = frame.style?.background_color || '#131313';
  captionDurationInput.value = frame.duration_ms || 2500;
  document.getElementById('add-caption').textContent = 'Save caption changes';
  setCaptionEditorOpen(true);
  updateCaptionPreview();
}

function setCaptionEditorOpen(open) {
  captionEditorPanel.hidden = !open;
  captionEditorToggle.setAttribute('aria-expanded', String(open));
  captionEditorToggle.querySelector('.disclosure-icon').textContent = open ? '−' : '＋';
  if (open) captionInput.focus();
}

function wireDisclosure(button, panel) {
  button.addEventListener('click', () => {
    const open = panel.hidden;
    panel.hidden = !open;
    button.setAttribute('aria-expanded', String(open));
    button.querySelector('.disclosure-icon').textContent = open ? '−' : '＋';
  });
}

function addGameTitleCard() {
  const metadata = state.parsedGame?.metadata;
  if (!metadata) {
    setStatus('Parse or import a PGN before adding a title card.', true);
    return;
  }
  const white = metadata.White || 'White';
  const black = metadata.Black || 'Black';
  const whiteRating = metadata.WhiteElo && metadata.WhiteElo !== '?' ? ` (${metadata.WhiteElo})` : '';
  const blackRating = metadata.BlackElo && metadata.BlackElo !== '?' ? ` (${metadata.BlackElo})` : '';
  const date = metadata.Date && metadata.Date !== '?' ? formatGameDate(metadata.Date) : '';
  const platform = detectGamePlatform(metadata);
  const platformLine = platform === 'lichess'
    ? 'Played on lichess.org'
    : platform === 'chesscom' ? 'Played on chess.com' : '';
  const titleLines = [];
  if (platformLine) titleLines.push(platformLine, '');
  titleLines.push(`${white}${whiteRating}`, 'vs', `${black}${blackRating}`);
  if (date) titleLines.push('', date);
  const text = titleLines.join('\n');
  state.timeline.forEach((frame) => { frame.sort_order = (frame.sort_order || 0) + 1; });
  state.timeline.unshift({
    type: 'caption',
    duration_ms: 2500,
    text,
    style: {
      font_family: captionFontInput.value || 'Noto Sans',
      font_size: Math.min(72, Math.max(28, Number(captionFontSizeInput.value) || 56)),
      font_weight: 700,
      padding: Math.min(120, Math.max(24, Number(captionPaddingInput.value) || 48)),
      horizontal_align: 'center',
      vertical_align: 'middle',
      text_color: captionColorInput.value,
      background_color: captionBackgroundInput.value,
      line_height: 1.2,
      platform,
    },
    sort_order: 1,
  });
  renderTimeline();
  setStatus('Automatic game title card added as frame 1.');
}

function formatGameDate(value) {
  const match = String(value).match(/^(\d{4})[.\-/](\d{2})[.\-/](\d{2})$/);
  if (!match || match[2] === '??' || match[3] === '??') return value;
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat('en-US', { month: 'long', day: 'numeric', year: 'numeric', timeZone: 'UTC' }).format(date);
}

function detectGamePlatform(metadata) {
  const source = `${metadata.Site || ''} ${metadata.Event || ''}`.toLowerCase();
  if (source.includes('lichess')) return 'lichess';
  if (source.includes('chess.com') || source.includes('chesscom')) return 'chesscom';
  return null;
}

function playerBarName(color) {
  const metadata = state.parsedGame?.metadata || {};
  const name = metadata[color] || color;
  const rating = metadata[`${color}Elo`];
  return rating && rating !== '?' ? `${name} (${rating})` : name;
}

function updateCaptionPreview() {
  const text = captionInput.value;
  captionLiveSection.hidden = !text.trim();
  captionCharacterCount.textContent = `${text.length} character${text.length === 1 ? '' : 's'}`;
  captionLiveText.textContent = text || 'Your caption preview will appear here';
  captionLivePreview.style.fontFamily = `'${captionFontInput.value || 'Noto Sans'}', sans-serif`;
  captionLivePreview.style.fontSize = `${Math.min(70, Math.max(12, Number(captionFontSizeInput.value) || 56))}px`;
  captionLivePreview.style.color = captionColorInput.value;
  captionLivePreview.style.backgroundColor = captionBackgroundInput.value;
  captionLivePreview.style.textAlign = captionAlignInput.value;
  captionLivePreview.style.padding = `${Math.min(100, Math.max(0, Number(captionPaddingInput.value) || 0))}px`;
  captionLivePreview.style.justifyContent = captionAlignInput.value === 'center' ? 'center' : 'flex-start';
}

function addMediaFrame() {
  const file = mediaFileInput.files?.[0];
  if (!file) {
    setStatus('Choose a picture or GIF first.', true);
    return;
  }
  if (file.size > 15 * 1024 * 1024) {
    setStatus('Media files must be 15 MB or smaller.', true);
    return;
  }
  const reader = new FileReader();
  reader.onload = () => {
    pendingMedia = { dataUrl: reader.result, name: file.name, type: file.type };
    mediaEditorImage.src = reader.result;
    mediaEditorImage.onerror = () => setStatus('This phone photo format cannot be decoded by your browser. Export it as JPEG or PNG first.', true);
    resetMediaEditor();
    mediaEditor.showModal();
  };
  reader.onerror = () => setStatus('Could not read that media file.', true);
  reader.readAsDataURL(file);
}

function confirmMediaFrame() {
  if (!pendingMedia) return;
  const addFrame = (dataUrl) => {
    state.timeline.forEach((frame) => {
      frame.sort_order = (frame.sort_order || 0) + 1;
    });
    state.timeline.unshift({
      type: 'media',
      duration_ms: Math.min(30000, Math.max(20, Number(mediaDurationInput.value) || 2000)),
      data_url: dataUrl,
      name: pendingMedia.name,
      scale: Number(mediaScaleInput.value) / 100,
      offset_x: Number(mediaOffsetXInput.value),
      offset_y: Number(mediaOffsetYInput.value),
      sort_order: 1,
    });
    renderTimeline();
    mediaFileInput.value = '';
    setStatus(`${pendingMedia.name} added to the timeline.`);
    pendingMedia = null;
    mediaEditor.close();
  };
  if (pendingMedia.type === 'image/gif' || pendingMedia.name.toLowerCase().endsWith('.gif')) {
    addFrame(pendingMedia.dataUrl);
    return;
  }
  try {
    const canvas = document.createElement('canvas');
    const reduction = Math.min(1, 1920 / Math.max(mediaEditorImage.naturalWidth, mediaEditorImage.naturalHeight));
    canvas.width = Math.round(mediaEditorImage.naturalWidth * reduction);
    canvas.height = Math.round(mediaEditorImage.naturalHeight * reduction);
    if (!canvas.width || !canvas.height) throw new Error('unsupported image format');
    canvas.getContext('2d').drawImage(mediaEditorImage, 0, 0, canvas.width, canvas.height);
    addFrame(canvas.toDataURL('image/jpeg', 0.92));
  } catch (_) {
    setStatus('Could not convert this photo. Please export the iPhone photo as JPEG or PNG and try again.', true);
  }
}

function updateMediaPreview() {
  mediaEditorImage.style.transform = `translate(${mediaOffsetXInput.value}%, ${mediaOffsetYInput.value}%) scale(${Number(mediaScaleInput.value) / 100})`;
}

function resetMediaEditor() {
  mediaScaleInput.value = '100';
  mediaOffsetXInput.value = '0';
  mediaOffsetYInput.value = '0';
  updateMediaPreview();
}

async function generateGif() {
  if (previewInProgress) return;
  if (!state.timeline.length) {
    setStatus('Build a timeline first.', true);
    return;
  }

  const requestBody = {
    white: playerBarName('White'),
    black: playerBarName('Black'),
    orientation: getExportOrientation(),
    coordinates: showCoordinatesInput.checked ? 'yes' : 'no',
    include_player_bars: showPlayerBarsInput.checked,
    preview: true,
    dark_square_color: darkSquareColorInput.value,
    timeline: state.timeline,
  };

  previewInProgress = true;
  previewButton.disabled = true;
  previewButton.classList.add('is-loading');
  previewButton.textContent = 'Generating…';
  try {
    const response = await fetch('/compose.gif', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(requestBody),
    });
    if (!response.ok) {
      const payload = await response.json().catch(() => ({}));
      throw new Error(payload.error || 'GIF generation failed');
    }
    const blob = await response.blob();
    const objectUrl = URL.createObjectURL(blob);
    previewImage.src = objectUrl;
    previewImage.onload = () => URL.revokeObjectURL(objectUrl);
    setStatus('GIF generated.');
  } catch (error) {
    setStatus(error.message, true);
  } finally {
    previewInProgress = false;
    previewButton.disabled = false;
    previewButton.classList.remove('is-loading');
    previewButton.textContent = 'Preview GIF';
  }
}

async function downloadGif() {
  if (downloadInProgress) return;
  if (!state.timeline.length) {
    setStatus('Build a timeline first.', true);
    return;
  }

  const requestBody = {
    white: playerBarName('White'),
    black: playerBarName('Black'),
    orientation: getExportOrientation(),
    coordinates: showCoordinatesInput.checked ? 'yes' : 'no',
    include_player_bars: showPlayerBarsInput.checked,
    preview: false,
    dark_square_color: darkSquareColorInput.value,
    timeline: state.timeline,
  };

  downloadInProgress = true;
  downloadButton.disabled = true;
  downloadButton.classList.add('is-loading');
  downloadButton.textContent = 'Preparing…';
  try {
    const response = await fetch('/compose.gif', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(requestBody),
    });
    if (!response.ok) {
      const payload = await response.json().catch(() => ({}));
      throw new Error(payload.error || 'Download failed');
    }
    const blob = await response.blob();
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = 'chessclip.gif';
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    setStatus('GIF downloaded.');
  } catch (error) {
    setStatus(error.message, true);
  } finally {
    downloadInProgress = false;
    downloadButton.disabled = false;
    downloadButton.classList.remove('is-loading');
    downloadButton.textContent = 'Download GIF';
  }
}

function getExportOrientation() {
  return document.querySelector('input[name="export-orientation"]:checked')?.value || 'white';
}

document.getElementById('import-game').addEventListener('click', importAndBuild);
lichessInput.addEventListener('input', () => {
  const source = lichessInput.value.trim();
  const isUsername = Boolean(source) && !source.includes('://') && !source.includes('/');
  gameHistoryOffsetInput.hidden = !isUsername;
  usernameSiteInput.hidden = true;
  if (isUsername) {
    const normalized = source.toLowerCase();
    const onLichess = usernameList(lichessUsernamesInput).some((name) => name.toLowerCase() === normalized);
    const onChesscom = usernameList(chesscomUsernamesInput).some((name) => name.toLowerCase() === normalized);
    usernameSiteInput.hidden = !(onLichess && onChesscom);
    if (onChesscom && !onLichess) usernameSiteInput.value = 'chesscom';
    if (onLichess && !onChesscom) usernameSiteInput.value = 'lichess';
  }
  renderPlayerSuggestions();
});
lichessInput.addEventListener('focus', renderPlayerSuggestions);
lichessInput.addEventListener('blur', () => { playerSuggestions.hidden = true; });
lichessInput.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') playerSuggestions.hidden = true;
});
document.getElementById('add-caption').addEventListener('click', addCaptionFrame);
captionEditorToggle.addEventListener('click', () => setCaptionEditorOpen(captionEditorPanel.hidden));
wireDisclosure(moveRangeToggle, moveRangePanel);
wireDisclosure(mediaPanelToggle, mediaPanel);
document.getElementById('add-title-card').addEventListener('click', addGameTitleCard);
document.getElementById('add-media').addEventListener('click', addMediaFrame);
document.getElementById('sort-timeline').addEventListener('click', sortTimeline);
document.getElementById('media-editor-add').addEventListener('click', confirmMediaFrame);
document.getElementById('media-editor-close').addEventListener('click', () => mediaEditor.close());
document.getElementById('media-editor-reset').addEventListener('click', resetMediaEditor);
document.getElementById('account-settings-open').addEventListener('click', () => accountSettings.showModal());
document.getElementById('account-settings-close').addEventListener('click', () => accountSettings.close());
document.getElementById('save-accounts').addEventListener('click', saveAccounts);
lichessUsernamesInput.addEventListener('input', () => refreshDefaultPlayers(defaultLichessInput, usernameList(lichessUsernamesInput), defaultLichessInput.value));
chesscomUsernamesInput.addEventListener('input', () => refreshDefaultPlayers(defaultChesscomInput, usernameList(chesscomUsernamesInput), defaultChesscomInput.value));
[mediaScaleInput, mediaOffsetXInput, mediaOffsetYInput].forEach((input) => input.addEventListener('input', updateMediaPreview));
[captionInput, captionFontInput, captionFontSizeInput, captionAlignInput, captionPaddingInput, captionColorInput, captionBackgroundInput].forEach((input) => input.addEventListener('input', updateCaptionPreview));
showPlayerBarsInput.addEventListener('change', updatePreviewDimensions);
showMoveHighlightsInput.addEventListener('change', buildTimeline);
document.getElementById('caption-settings-open').addEventListener('click', () => captionSettings.showModal());
document.getElementById('caption-settings-close').addEventListener('click', () => captionSettings.close());
document.getElementById('caption-settings-done').addEventListener('click', () => captionSettings.close());
captionSettings.addEventListener('click', (event) => {
  if (event.target === captionSettings) captionSettings.close();
});
moveStartInput.addEventListener('change', buildTimeline);
moveEndInput.addEventListener('change', buildTimeline);
moveDelayInput.addEventListener('change', buildTimeline);
document.getElementById('preview-gif').addEventListener('click', generateGif);
document.getElementById('download-gif').addEventListener('click', downloadGif);

renderTimeline();
loadAccounts();
fetch('/google-fonts.json').then((response) => response.json()).then((fonts) => {
  captionFontInput.innerHTML = '';
  fonts.forEach((font) => {
    const option = document.createElement('option');
    option.value = font;
    option.textContent = font;
    captionFontInput.appendChild(option);
  });
  updateCaptionPreview();
}).catch(() => { captionFontInput.innerHTML = '<option>Noto Sans</option>'; });
updateCaptionPreview();
updatePreviewDimensions();
