const NEW_GAME_PLACEHOLDER_ROWS = 4;

let players: any[] = [];
let editingGameId: string | null = null;

async function api(path: string, opts: any = {}) {
  const res = await fetch('/api' + path, {
    headers: { 'Content-Type': 'application/json' },
    ...opts,
    body: opts.body ? JSON.stringify(opts.body) : undefined
  });
  if (res.status === 204) return null;
  if (!res.ok) {
    const text = await res.text();
    throw new Error(text || res.statusText);
  }
  return res.json();
}

function formatMoney(cents: number) {
  const dollars = Math.abs(cents / 100).toFixed(2);
  if (cents >= 0) return '$' + dollars;
  return '-$' + dollars;
}

function formatDate(d: string) {
  const date = new Date(d);
  return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

function showTab(name: string) {
  document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
  document.querySelectorAll('[id$="-tab"]').forEach(t => (t as HTMLElement).style.display = 'none');
  document.querySelector(`[onclick="showTab('${name}')"]`)!.classList.add('active');
  document.getElementById(name + '-tab')!.style.display = 'block';
}
(window as any).showTab = showTab;

async function loadStats() {
  try {
    const stats = await api('/stats');
    if (!stats || stats.length === 0) {
      document.getElementById('stats-body')!.innerHTML = '<tr><td colspan="3" class="muted">No data yet</td></tr>';
      return;
    }
    document.getElementById('stats-body')!.innerHTML = stats.map((s: any) => {
      const dupes = stats.filter((st: any) => st.player.first_name === s.player.first_name && st.player.last_name === s.player.last_name);
      const name = dupes.length > 1
        ? `${s.player.first_name} ${s.player.last_name} (${s.player.id.slice(-6)})`
        : `${s.player.first_name} ${s.player.last_name}`;
      return `<tr><td>${name}</td><td>${s.total_games}</td><td class="${s.net_cents >= 0 ? 'positive' : 'negative'}">${formatMoney(s.net_cents)}</td></tr>`;
    }).join('');
  } catch {
    document.getElementById('stats-body')!.innerHTML = '<tr><td colspan="3" class="muted">Failed to load</td></tr>';
  }
}

async function loadGames() {
  try {
    const games = await api('/games');
    if (!games || games.length === 0) {
      document.getElementById('games-body')!.innerHTML = '<tr><td colspan="3" class="muted">No games yet</td></tr>';
      return;
    }
    document.getElementById('games-body')!.innerHTML = games.map((g: any) => {
      const balanced = g.pot_cents === g.payout_cents;
      const icon = balanced ? '<span class="positive">\u2713</span>' : '<span class="negative">\u2717</span>';
      const inProgress = !g.ended_at ? ' <span class="muted" style="font-size:0.8rem">(in progress)</span>' : '';
      const actions = g.settled
        ? `<button class="btn-secondary small" onclick="viewGame('${g.id}')">View</button>
           <span class="muted" style="font-size: 0.8rem; padding: 4px 8px;">Settled</span>`
        : `<button class="btn-secondary small" onclick="viewGame('${g.id}')">View</button>
           <button class="btn-secondary small" onclick="editGame('${g.id}')">Edit</button>`;
      return `<tr><td>${icon} ${formatDate(g.started_at)}${inProgress}</td><td>${formatMoney(g.pot_cents)}</td><td>${actions}</td></tr>`;
    }).join('');
  } catch {
    document.getElementById('games-body')!.innerHTML = '<tr><td colspan="3" class="muted">Failed to load</td></tr>';
  }
}

async function loadPlayers() {
  try {
    players = await api('/players') || [];
    if (players.length === 0) {
      document.getElementById('players-body')!.innerHTML = '<tr><td class="muted">No players yet</td></tr>';
      return;
    }
    document.getElementById('players-body')!.innerHTML = players.map((p: any) => {
      const dupes = players.filter((pl: any) => pl.first_name === p.first_name && pl.last_name === p.last_name);
      const name = dupes.length > 1
        ? `${p.first_name} ${p.last_name} (${p.id.slice(-6)})`
        : `${p.first_name} ${p.last_name}`;
      return `<tr><td>${name}</td></tr>`;
    }).join('');
  } catch {
    document.getElementById('players-body')!.innerHTML = '<tr><td class="muted">Failed to load</td></tr>';
  }
}

function openModal(id: string) { document.getElementById(id)!.classList.add('active'); }
function closeModal(id: string) { document.getElementById(id)!.classList.remove('active'); }
(window as any).closeModal = closeModal;

function getPlayerDisplayName(player: any) {
  const dupes = players.filter(p => p.first_name === player.first_name);
  if (dupes.length > 1) return `${player.first_name} ${player.last_name} (${player.id.slice(-6)})`;
  return player.first_name;
}

function openPlayerModal() {
  (document.getElementById('player-first') as HTMLInputElement).value = '';
  (document.getElementById('player-last') as HTMLInputElement).value = '';
  document.getElementById('player-warning')!.style.display = 'none';
  openModal('player-modal');
}
(window as any).openPlayerModal = openPlayerModal;

async function createPlayer(force = false) {
  const first = (document.getElementById('player-first') as HTMLInputElement).value.trim();
  const last = (document.getElementById('player-last') as HTMLInputElement).value.trim();
  if (!first || !last) {
    document.getElementById('player-warning')!.textContent = 'Please enter both names';
    document.getElementById('player-warning')!.style.display = 'block';
    return;
  }
  try {
    const res = await fetch('/api/players', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ first_name: first, last_name: last, force })
    });
    if (res.status === 409) {
      document.getElementById('player-warning')!.textContent = 'Player exists. Click again to add anyway.';
      document.getElementById('player-warning')!.style.display = 'block';
      document.querySelector('#player-modal .actions button:last-child')!.setAttribute('onclick', 'createPlayer(true)');
      return;
    }
    closeModal('player-modal');
    loadPlayers();
    loadStats();
  } catch {
    document.getElementById('player-warning')!.textContent = 'Failed to add player';
    document.getElementById('player-warning')!.style.display = 'block';
  }
}
(window as any).createPlayer = createPlayer;

function getTodayDate() {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
}

function addEntryRow(playerId = '', buyIn = 20, winnings = 0) {
  const empty = document.querySelector('#entries-container .empty-msg');
  if (empty) empty.remove();

  const div = document.createElement('div');
  div.className = 'entry-row';
  div.innerHTML = `
    <select class="entry-player" onchange="updateBalance()">
      <option value="">- SELECT -</option>
      ${players.map(p => `<option value="${p.id}" ${p.id === playerId ? 'selected' : ''}>${getPlayerDisplayName(p)}</option>`).join('')}
    </select>
    <input type="number" class="entry-buyin" placeholder="In" value="${buyIn}" oninput="updateBalance()">
    <input type="number" class="entry-winnings" placeholder="Out" value="${winnings}" oninput="updateBalance()">
    <button class="btn-remove" onclick="removeEntry(this)">\u00d7</button>
  `;
  document.getElementById('entries-container')!.appendChild(div);
  updateBalance();
}
(window as any).addEntryRow = addEntryRow;

function updateBalance() {
  const rows = document.querySelectorAll('.entry-row');
  if (rows.length === 0) {
    document.getElementById('balance-check')!.style.display = 'none';
    return;
  }

  let totalIn = 0, totalOut = 0;
  rows.forEach(row => {
    totalIn += parseFloat((row.querySelector('.entry-buyin') as HTMLInputElement).value) || 0;
    totalOut += parseFloat((row.querySelector('.entry-winnings') as HTMLInputElement).value) || 0;
  });

  const balanceEl = document.getElementById('balance-check')!;
  const diff = totalIn - totalOut;
  if (Math.abs(diff) < 0.01) {
    balanceEl.className = 'balance-check valid';
    balanceEl.textContent = `\u2713 Balanced - Pot: $${totalIn.toFixed(2)}`;
  } else {
    balanceEl.className = 'balance-check invalid';
    const remaining = diff > 0 ? `$${diff.toFixed(2)} left to pay out` : `$${Math.abs(diff).toFixed(2)} extra paid out`;
    balanceEl.textContent = `Pot: $${totalIn.toFixed(2)} | Paid: $${totalOut.toFixed(2)} | ${remaining}`;
  }
  balanceEl.style.display = 'block';
}
(window as any).updateBalance = updateBalance;

function removeEntry(btn: HTMLElement) {
  btn.parentElement!.remove();
  if (document.querySelectorAll('.entry-row').length === 0) {
    document.getElementById('entries-container')!.innerHTML = '<div class="empty-msg">Click "+ Add" to add players</div>';
    document.getElementById('balance-check')!.style.display = 'none';
  } else {
    updateBalance();
  }
}
(window as any).removeEntry = removeEntry;

function openGameModal() {
  editingGameId = null;
  document.getElementById('game-modal-title')!.textContent = 'New Game';
  (document.getElementById('game-date') as HTMLInputElement).value = getTodayDate();
  (document.getElementById('game-start-time') as HTMLInputElement).value = '';
  (document.getElementById('game-end-time') as HTMLInputElement).value = '';
  document.getElementById('entries-container')!.innerHTML = '';
  document.getElementById('game-error')!.style.display = 'none';
  document.getElementById('balance-check')!.style.display = 'none';
  for (let i = 0; i < NEW_GAME_PLACEHOLDER_ROWS; i++) addEntryRow('', 20, 0);
  openModal('game-modal');
}
(window as any).openGameModal = openGameModal;

async function editGame(id: string) {
  editingGameId = id;
  try {
    const game = await api('/games/' + id);
    document.getElementById('game-modal-title')!.textContent = 'Edit Game';
    const startDate = new Date(game.game.started_at);
    (document.getElementById('game-date') as HTMLInputElement).value = `${startDate.getFullYear()}-${String(startDate.getMonth() + 1).padStart(2, '0')}-${String(startDate.getDate()).padStart(2, '0')}`;
    (document.getElementById('game-start-time') as HTMLInputElement).value = startDate.toTimeString().slice(0, 5);
    if (game.game.ended_at) {
      (document.getElementById('game-end-time') as HTMLInputElement).value = new Date(game.game.ended_at).toTimeString().slice(0, 5);
    } else {
      (document.getElementById('game-end-time') as HTMLInputElement).value = '';
    }
    document.getElementById('entries-container')!.innerHTML = '';
    game.entries.forEach((e: any) => addEntryRow(e.player.id, e.entry.buy_in_cents / 100, e.entry.winnings_cents / 100));
    if (game.entries.length === 0) {
      document.getElementById('entries-container')!.innerHTML = '<div class="empty-msg">Click "+ Add" to add players</div>';
      document.getElementById('balance-check')!.style.display = 'none';
    }
    document.getElementById('game-error')!.style.display = 'none';
    openModal('game-modal');
  } catch { alert('Failed to load game'); }
}
(window as any).editGame = editGame;

async function viewGame(id: string) {
  try {
    const game = await api('/games/' + id);
    const startDate = new Date(game.game.started_at);
    const dateStr = startDate.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' });
    const startTime = startDate.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
    if (game.game.ended_at) {
      const endTime = new Date(game.game.ended_at).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
      document.getElementById('view-game-date')!.textContent = `${dateStr} \u2022 ${startTime} - ${endTime}`;
    } else {
      document.getElementById('view-game-date')!.textContent = `${dateStr} \u2022 ${startTime} - In progress`;
    }

    let totalIn = 0, totalOut = 0;
    game.entries.forEach((e: any) => { totalIn += e.entry.buy_in_cents; totalOut += e.entry.winnings_cents; });

    const balanceEl = document.getElementById('view-balance')!;
    if (totalIn === totalOut) {
      balanceEl.className = 'balance-check valid';
      balanceEl.textContent = `\u2713 Balanced - Pot: ${formatMoney(totalIn)}`;
    } else {
      balanceEl.className = 'balance-check invalid';
      const diff = totalIn - totalOut;
      const msg = diff > 0 ? `${formatMoney(diff)} unpaid` : `${formatMoney(Math.abs(diff))} overpaid`;
      balanceEl.textContent = `\u2717 Unbalanced - Pot: ${formatMoney(totalIn)}, Paid: ${formatMoney(totalOut)} (${msg})`;
    }

    const sorted = [...game.entries].sort((a: any, b: any) => b.entry.winnings_cents - a.entry.winnings_cents);
    document.getElementById('view-entries')!.innerHTML = sorted.map((e: any) => {
      const net = e.entry.winnings_cents - e.entry.buy_in_cents;
      const netClass = net > 0 ? 'positive' : (net < 0 ? 'negative' : '');
      const dupes = players.filter(p => p.first_name === e.player.first_name);
      const name = dupes.length > 1
        ? `${e.player.first_name} ${e.player.last_name} (${e.player.id.slice(-6)})`
        : `${e.player.first_name} ${e.player.last_name}`;
      return `<tr><td>${name}</td><td>${formatMoney(e.entry.buy_in_cents)}</td><td>${formatMoney(e.entry.winnings_cents)}</td><td class="${netClass}">${net >= 0 ? '+' : ''}${formatMoney(net)}</td></tr>`;
    }).join('');

    openModal('view-modal');
  } catch { alert('Failed to load game'); }
}
(window as any).viewGame = viewGame;

async function saveGame() {
  const errEl = document.getElementById('game-error')!;
  errEl.style.display = 'none';
  const date = (document.getElementById('game-date') as HTMLInputElement).value || getTodayDate();
  const startTime = (document.getElementById('game-start-time') as HTMLInputElement).value;
  const endTime = (document.getElementById('game-end-time') as HTMLInputElement).value;
  if (!startTime) { errEl.textContent = 'Please enter a start time'; errEl.style.display = 'block'; return; }

  const entries = Array.from(document.querySelectorAll('.entry-row')).map(row => ({
    player_id: (row.querySelector('.entry-player') as HTMLSelectElement).value,
    buy_in_cents: Math.round((parseFloat((row.querySelector('.entry-buyin') as HTMLInputElement).value) || 0) * 100),
    winnings_cents: Math.round((parseFloat((row.querySelector('.entry-winnings') as HTMLInputElement).value) || 0) * 100)
  })).filter(e => e.player_id);

  if (entries.length === 0) { errEl.textContent = 'Please add at least one player'; errEl.style.display = 'block'; return; }

  const totalIn = entries.reduce((sum, e) => sum + e.buy_in_cents, 0);
  const totalOut = entries.reduce((sum, e) => sum + e.winnings_cents, 0);
  if (totalIn !== totalOut) {
    const diff = (totalIn - totalOut) / 100;
    const msg = diff > 0 ? `$${diff.toFixed(2)} left to pay out` : `$${Math.abs(diff).toFixed(2)} overpaid`;
    if (!confirm(`Money doesn't balance (${msg}). Save anyway?`)) return;
  }

  const startedAt = new Date(date + 'T' + startTime + ':00').toISOString();
  const endedAt = endTime ? new Date(date + 'T' + endTime + ':00').toISOString() : null;
  const body = { started_at: startedAt, ended_at: endedAt, entries };

  try {
    if (editingGameId) {
      await api('/games/' + editingGameId, { method: 'PUT', body });
    } else {
      await api('/games', { method: 'POST', body });
    }
    closeModal('game-modal');
    loadGames();
    loadStats();
  } catch {
    errEl.textContent = 'Failed to save game';
    errEl.style.display = 'block';
  }
}
(window as any).saveGame = saveGame;

// init
loadStats();
loadGames();
loadPlayers();
