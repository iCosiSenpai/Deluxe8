// Italian. Keys are the English source text (see ../i18n.svelte.ts); one file per area of the launcher.

import { components } from './it/components';
import { graphics } from './it/graphics';
import { home } from './it/home';
import { labels } from './it/labels';
import { online } from './it/online';
import { pages } from './it/pages';
import { setup } from './it/setup';
import { state } from './it/state';
import { system } from './it/system';

export const it: Record<string, string> = { ...components, ...graphics, ...home, ...labels, ...online, ...pages, ...setup, ...state, ...system };
