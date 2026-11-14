// The overlay page's drawing, checked in a browser: three cells of each kind
// that is drawn, the badge at the bottom right, drawn twice then cleared.
import { draw } from './calque.js';

const errors = [];
window.addEventListener('error', (event) => errors.push(String(event.message)));
const svg = document.getElementById('calque');
const drawing = {
  cells: [
    { points: [[400, 100], [432, 116], [400, 132], [368, 116]], kind: 'obstacle' },
    { points: [[464, 100], [496, 116], [464, 132], [432, 116]], kind: 'hole' },
    { points: [[528, 100], [560, 116], [528, 132], [496, 116]], kind: 'undecided' },
  ],
  badge: 'Carte non vérifiée · 1 combat sur 2',
};
draw(svg, drawing);
draw(svg, drawing);
const badge = svg.querySelector('.badge');
const area = svg.getBoundingClientRect();
const result = {
  errors,
  kinds: [...svg.querySelectorAll('polygon')].map((p) => p.getAttribute('class')),
  marks: [...svg.querySelectorAll('.mark')].map((t) => t.textContent),
  badge: badge ? badge.textContent : null,
  badgeWidth: badge ? Number(badge.querySelector('rect').getAttribute('width')) : 0,
  textLength: badge ? badge.querySelector('text').getComputedTextLength() : 0,
  badgeAt: badge ? badge.getAttribute('transform') : null,
  area: [area.width, area.height],
};
draw(svg, { cells: [], badge: null });
result.cleared = svg.childElementCount;
const out = document.createElement('pre');
out.id = 'calque-essai';
out.textContent = JSON.stringify(result);
document.body.append(out);
