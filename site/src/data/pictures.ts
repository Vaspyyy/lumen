// Small pictures for the thumbnails on the front page, drawn here so the
// page loads no photos. `id` keeps the gradients of two copies on one page apart.
export function thumbnails(id: string) {
  return [
  {
    name: 'harbor.jpg',
    svg: `<svg viewBox="0 0 160 110" preserveAspectRatio="xMidYMid slice"><defs><linearGradient id="${id}1a" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2a1a4e"/><stop offset=".6" stop-color="#c9577b"/><stop offset="1" stop-color="#ffbb6b"/></linearGradient><linearGradient id="${id}1b" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#8a4172"/><stop offset="1" stop-color="#1b1337"/></linearGradient></defs><rect width="160" height="66" fill="url(#${id}1a)"/><circle cx="106" cy="62" r="16" fill="#ffe3a8"/><rect y="64" width="160" height="46" fill="url(#${id}1b)"/><g fill="#ffd9a0" opacity=".75"><rect x="93" y="70" width="26" height="2" rx="1"/><rect x="98" y="77" width="16" height="2" rx="1"/><rect x="89" y="85" width="32" height="2" rx="1"/><rect x="101" y="94" width="10" height="2" rx="1"/></g><path d="M20 64l7-17 2 17zM33 64l11-26 3 26z" fill="#170f2c"/><rect x="12" y="63" width="46" height="3" rx="1.5" fill="#170f2c"/></svg>`,
  },
  {
    name: 'dunes.jpg',
    svg: `<svg viewBox="0 0 160 110" preserveAspectRatio="xMidYMid slice"><defs><linearGradient id="${id}2a" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffd9b0"/><stop offset="1" stop-color="#ff9d6e"/></linearGradient></defs><rect width="160" height="110" fill="url(#${id}2a)"/><circle cx="118" cy="30" r="11" fill="#fff3d6"/><path d="M0 62q40-26 82-6t78-12v66H0z" fill="#e8935a"/><path d="M0 80q52-30 96-8t64-10v48H0z" fill="#c96a3d"/><path d="M0 98q60-24 110-6t50-4v22H0z" fill="#8c4127"/></svg>`,
  },
  {
    name: 'summit.jpg',
    svg: `<svg viewBox="0 0 160 110" preserveAspectRatio="xMidYMid slice"><defs><linearGradient id="${id}3a" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#9dbcff"/><stop offset="1" stop-color="#eef3ff"/></linearGradient></defs><rect width="160" height="110" fill="url(#${id}3a)"/><path d="M0 84l34-40 26 26 30-44 40 46 30-22v60H0z" fill="#7f95d8"/><path d="M90 26l-11 16 8-3 5 6 6-5 6 4z" fill="#fff"/><path d="M0 110V92l40-34 30 26 34-30 56 44v12z" fill="#39488c"/><path d="M40 58l-10 9 9-2 5 5 4-6z" fill="#e9efff"/></svg>`,
  },
  {
    name: 'lanterns.jpg',
    svg: `<svg viewBox="0 0 160 110" preserveAspectRatio="xMidYMid slice"><defs><radialGradient id="${id}4a"><stop offset="0" stop-color="#ffe2a3"/><stop offset=".5" stop-color="#ff9d4d" stop-opacity=".85"/><stop offset="1" stop-color="#ff7a3d" stop-opacity="0"/></radialGradient></defs><rect width="160" height="110" fill="#170f26"/><path d="M0 22q80 26 160 0M0 52q80 22 160-4" stroke="#3a2a55" stroke-width="1" fill="none"/><g fill="url(#${id}4a)"><circle cx="24" cy="34" r="17"/><circle cx="66" cy="44" r="22"/><circle cx="112" cy="40" r="18"/><circle cx="146" cy="30" r="14"/><circle cx="40" cy="74" r="20"/><circle cx="92" cy="78" r="24"/><circle cx="136" cy="72" r="18"/></g><path d="M0 110V96h160v14z" fill="#0e0918"/></svg>`,
  },
  {
    name: 'pines.jpg',
    svg: `<svg viewBox="0 0 160 110" preserveAspectRatio="xMidYMid slice"><defs><linearGradient id="${id}5a" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#d6ece2"/><stop offset="1" stop-color="#86bba8"/></linearGradient></defs><rect width="160" height="110" fill="url(#${id}5a)"/><path d="M0 70q40-22 80-6t80-14v60H0z" fill="#5b9a86"/><path d="M0 86q46-20 90-4t70-10v38H0z" fill="#2f6b5c"/><g fill="#143a33"><path d="M22 110l10-34 10 34zM46 110l8-26 8 26zM104 110l11-38 11 38zM130 110l8-24 8 24zM72 110l7-20 7 20z"/></g></svg>`,
  },
];
}
