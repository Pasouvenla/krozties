// Reconnaître un lien DofusBook : trouver le numéro d'équipement dans ce que le
// joueur colle.

export const CODE62 = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';

/// Retrouve le numéro d'équipement dans un lien DofusBook, court ou long.
export function decoderLienCourt(lien) {
  const m = String(lien).match(/d-bk\.net\/[a-z]{2}\/d\/([0-9A-Za-z]+)/)
    || String(lien).match(/equipement\/(\d+)/);
  if (!m) return null;
  if (/^\d+$/.test(m[1]) && m[1].length > 6) return Number(m[1]);
  return [...m[1]].reduce((v, c) => v * 62 + CODE62.indexOf(c), 0);
}
