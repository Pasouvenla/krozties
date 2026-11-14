// The overlay over the game window: draws what Rust sends, layout B.

const SVG = 'http://www.w3.org/2000/svg';

function node(name, attributes) {
  const element = document.createElementNS(SVG, name);
  for (const [key, value] of Object.entries(attributes)) {
    element.setAttribute(key, String(value));
  }
  return element;
}

// Draws `drawing` ({ cells: [{ points, kind }], badge }) in `svg`, in place of
// what it showed.
export function draw(svg, drawing) {
  svg.replaceChildren();
  for (const cell of drawing.cells) {
    const points = cell.points.map(([x, y]) => `${x},${y}`).join(' ');
    svg.append(node('polygon', { points, class: cell.kind }));
    if (cell.kind === 'undecided') {
      const [top, right, bottom, left] = cell.points;
      const size = Math.max(8, Math.min(16, (bottom[1] - top[1]) * 0.6));
      const mark = node('text', { x: (left[0] + right[0]) / 2, y: (top[1] + bottom[1]) / 2, class: 'mark', 'font-size': size });
      mark.textContent = '?';
      svg.append(mark);
    }
  }
  if (drawing.badge) {
    const badge = node('g', { class: 'badge' });
    const box = node('rect', { width: 252, height: 30, rx: 6 });
    const text = node('text', { x: 30, y: 20 });
    text.textContent = drawing.badge;
    badge.append(box, node('circle', { cx: 16, cy: 15, r: 5 }), text);
    svg.append(badge);
    // As wide as its text, at the bottom right of the window.
    const width = Math.ceil(text.getComputedTextLength()) + 44;
    box.setAttribute('width', String(width));
    const area = svg.getBoundingClientRect();
    badge.setAttribute('transform', `translate(${area.width - width - 14}, ${area.height - 44})`);
  }
}

const tauri = window.__TAURI__;
if (tauri) {
  const svg = document.getElementById('calque');
  tauri.event.listen('calque', (event) => draw(svg, event.payload));
}
