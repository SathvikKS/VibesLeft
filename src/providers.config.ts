import type { LucideIcon } from 'lucide-react';
import { Bot, MessageSquare } from 'lucide-react';

export interface ProviderConfig {
  id: string;
  name: string;
  icon: LucideIcon;
  accentColor: string;
}

export const AVAILABLE_PROVIDERS: ProviderConfig[] = [
  { id: 'claude', name: 'Claude',  icon: Bot,           accentColor: 'indigo'  },
  { id: 'codex',  name: 'ChatGPT', icon: MessageSquare, accentColor: 'emerald' },
];

export const ENABLED_PROVIDERS: ProviderConfig[] = AVAILABLE_PROVIDERS;
