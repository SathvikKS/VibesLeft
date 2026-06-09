import { Activity, LayoutDashboard, BarChart2 } from 'lucide-react';
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuBadge,
} from '@/components/ui/sidebar';
import { ENABLED_PROVIDERS } from '@/providers.config';
import type { Page } from '@/App';
import { Badge } from '@/components/ui/badge';

interface AppSidebarProps {
  page: Page;
  setPage: (page: Page) => void;
  providerUtilization?: Record<string, number>;
}

export const AppSidebar = ({ page, setPage, providerUtilization }: AppSidebarProps) => {
  return (
    <Sidebar>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" asChild>
              <button
                onClick={() => setPage({ view: 'dashboard' })}
                className="flex items-center gap-2"
              >
                <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
                  <Activity className="size-4" />
                </div>
                <div className="grid flex-1 text-left text-sm leading-tight">
                  <span className="truncate font-semibold">Vibes Left</span>
                </div>
              </button>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>Overview</SidebarGroupLabel>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton
                isActive={page.view === 'dashboard'}
                onClick={() => setPage({ view: 'dashboard' })}
                tooltip="Dashboard"
              >
                <LayoutDashboard className="size-4" />
                <span>Dashboard</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarGroup>

        <SidebarGroup>
          <SidebarGroupLabel>Providers</SidebarGroupLabel>
          <SidebarMenu>
            {ENABLED_PROVIDERS.map((provider) => {
              const utilization = providerUtilization?.[provider.id];
              const isActive = page.view === 'provider' && page.id === provider.id;

              return (
                <SidebarMenuItem key={provider.id}>
                  <SidebarMenuButton
                    isActive={isActive}
                    onClick={() => setPage({ view: 'provider', id: provider.id })}
                    tooltip={provider.name}
                  >
                    <provider.icon className="size-4" />
                    <span>{provider.name}</span>
                  </SidebarMenuButton>
                  {utilization !== undefined && (
                    <SidebarMenuBadge>
                      <Badge
                        variant="outline"
                        className={`flex items-center gap-1 ${
                          utilization >= 90
                            ? 'border-red-800/50 bg-red-950/40 text-red-400'
                            : utilization >= 75
                              ? 'border-amber-800/50 bg-amber-950/40 text-amber-400'
                              : 'border-emerald-800/50 bg-emerald-950/40 text-emerald-400'
                        }`}
                      >
                        <BarChart2 size={10} />
                        {utilization}%
                      </Badge>
                    </SidebarMenuBadge>
                  )}
                </SidebarMenuItem>
              );
            })}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
    </Sidebar>
  );
};
