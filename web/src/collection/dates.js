// @ts-check
// Names of weeks and months, for the group labels of the grid.

import { date, language } from '../i18n.js';
import { weekStart } from './layout.js';

/** "12–18 окт." or "28 сент. – 4 окт.". @param {number} ms */
export function weekLabel(ms) {
  const a = new Date(weekStart(ms)), b = new Date(a);
  b.setDate(b.getDate() + 6);
  const end = date(b.getTime(), { day: 'numeric', month: 'short' });
  return a.getMonth() === b.getMonth() ? `${a.getDate()}–${end}` : `${date(a.getTime(), { day: 'numeric', month: 'short' })} – ${end}`;
}

/** "Октябрь", with the year when it is not this one. @param {number} ms */
export function monthLabel(ms) {
  const d = new Date(ms);
  const opts = /** @type {Intl.DateTimeFormatOptions} */ ({ month: 'long', ...(d.getFullYear() !== new Date().getFullYear() ? { year: 'numeric' } : {}) });
  const s = d.toLocaleDateString(language() === 'ru' ? 'ru-RU' : 'en-GB', opts);
  return s.charAt(0).toUpperCase() + s.slice(1);
}
