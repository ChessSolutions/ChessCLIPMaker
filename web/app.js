const state = {
  parsedGame: null,
  timeline: [],
  selectedRange: [0, 0],
};

const pgnInput = document.getElementById('pgn');
const lichessInput = document.getElementById('lichess-url');
const moveStartInput = document.getElementById('move-start');
const moveEndInput = document.getElementById('move-end');
const moveDelayInput = document.getElementById('move-delay');
const captionInput = document.getElementById('caption-text');
const captionFontInput = document.getElementById('caption-font');
const captionColorInput = document.getElementById('caption-color');
const captionBackgroundInput = document.getElementById('caption-background');
const captionDurationInput = document.getElementById('caption-duration');
const captionSettings = document.getElementById('caption-settings');
const mediaFileInput = document.getElementById('media-file');
const mediaDurationInput = document.getElementById('media-duration');
const mediaEditor = document.getElementById('media-editor');
const mediaEditorImage = document.getElementById('media-editor-image');
const mediaScaleInput = document.getElementById('media-scale');
const mediaOffsetXInput = document.getElementById('media-offset-x');
const mediaOffsetYInput = document.getElementById('media-offset-y');
let pendingMedia = null;
const statusNode = document.getElementById('status');
const timelineList = document.getElementById('timeline-list');
const previewImage = document.getElementById('preview-image');

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
    label.textContent = `${index + 1}. ${frame.type === 'caption' ? `Caption: ${frame.text}` : frame.type === 'media' ? `Media: ${frame.name || 'image'}` : frame.move_label || 'Starting position'}`;
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
    remove.className = 'secondary';
    remove.textContent = 'Remove';
    remove.addEventListener('click', () => {
      state.timeline.splice(index, 1);
      renderTimeline();
    });
    item.appendChild(label);
    item.appendChild(duration);
    item.appendChild(up);
    item.appendChild(down);
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
  renderTimeline();
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
    setStatus(`Parsed ${totalMoves} moves and added them to the GIF timeline.`);
  } catch (error) {
    setStatus(error.message, true);
  }
}

async function importLichess() {
  const url = lichessInput.value.trim();
  if (!url) {
    setStatus('Enter a Lichess game URL.', true);
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
      throw new Error(payload.error || 'Lichess import failed');
    }
    pgnInput.value = payload.pgn;
    setStatus('Lichess game imported.');
    await parsePgn();
  } catch (error) {
    setStatus(error.message, true);
  }
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
  frames.push({
    type: 'board',
    duration_ms: moveDelay,
    fen: selectedMoves[0].fen_before,
    move_label: `Position before ${formatMoveLabel(selectedMoves[0])}`,
  });

  selectedMoves.forEach((move) => {
    frames.push({
      type: 'board',
      duration_ms: moveDelay,
      fen: move.fen_after,
      last_move: move.uci,
      check: move.is_check ? 'yes' : 'no',
      move_label: formatMoveLabel(move),
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
  state.timeline.push({
    type: 'caption',
    duration_ms: Math.min(30000, Math.max(20, Number(captionDurationInput.value) || 2000)),
    text: caption,
    style: {
      font_family: captionFontInput.value,
      font_size: 56,
      padding: 48,
      horizontal_align: 'center',
      text_color: captionColorInput.value,
      background_color: captionBackgroundInput.value,
    },
  });
  renderTimeline();
  captionInput.value = '';
  setStatus('Caption frame added.');
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
    pendingMedia = { dataUrl: reader.result, name: file.name };
    mediaEditorImage.src = reader.result;
    resetMediaEditor();
    mediaEditor.showModal();
  };
  reader.onerror = () => setStatus('Could not read that media file.', true);
  reader.readAsDataURL(file);
}

function confirmMediaFrame() {
  if (!pendingMedia) return;
  state.timeline.push({
      type: 'media',
      duration_ms: Math.min(30000, Math.max(20, Number(mediaDurationInput.value) || 2000)),
      data_url: pendingMedia.dataUrl,
      name: pendingMedia.name,
      scale: Number(mediaScaleInput.value) / 100,
      offset_x: Number(mediaOffsetXInput.value),
      offset_y: Number(mediaOffsetYInput.value),
    });
    renderTimeline();
    mediaFileInput.value = '';
    setStatus(`${pendingMedia.name} added to the timeline.`);
    pendingMedia = null;
    mediaEditor.close();
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
  if (!state.timeline.length) {
    setStatus('Build a timeline first.', true);
    return;
  }

  const requestBody = {
    white: 'White',
    black: 'Black',
    orientation: 'white',
    coordinates: 'yes',
    include_player_bars: true,
    preview: true,
    timeline: state.timeline,
  };

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
  }
}

async function downloadGif() {
  if (!state.timeline.length) {
    setStatus('Build a timeline first.', true);
    return;
  }

  const requestBody = {
    white: 'White',
    black: 'Black',
    orientation: 'white',
    coordinates: 'yes',
    include_player_bars: true,
    preview: false,
    timeline: state.timeline,
  };

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
  }
}

document.getElementById('parse-pgn').addEventListener('click', parsePgn);
document.getElementById('import-lichess').addEventListener('click', importLichess);
document.getElementById('build-timeline').addEventListener('click', buildTimeline);
document.getElementById('add-caption').addEventListener('click', addCaptionFrame);
document.getElementById('add-media').addEventListener('click', addMediaFrame);
document.getElementById('media-editor-add').addEventListener('click', confirmMediaFrame);
document.getElementById('media-editor-close').addEventListener('click', () => mediaEditor.close());
document.getElementById('media-editor-reset').addEventListener('click', resetMediaEditor);
[mediaScaleInput, mediaOffsetXInput, mediaOffsetYInput].forEach((input) => input.addEventListener('input', updateMediaPreview));
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
