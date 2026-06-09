import { useState, useEffect, useCallback } from 'react';
import { SidebarProvider, SidebarInset } from '@/components/ui/sidebar';
import { TooltipProvider } from '@/components/ui/tooltip';
import { AppSidebar } from '@/components/layout/AppSidebar';
import { Dashboard } from '@/pages/Dashboard';
import { ProviderDetail } from '@/pages/ProviderDetail';

export type Page =
  | { view: 'dashboard' }
  | { view: 'provider'; id: string };

function App() {
  const [page, setPage] = useState<Page>({ view: 'dashboard' });
  const [providerUtilization, setProviderUtilization] = useState<Record<string, number>>({});

  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = (dark: boolean) =>
      document.documentElement.classList.toggle('dark', dark);
    apply(mq.matches);
    mq.addEventListener('change', e => apply(e.matches));
    return () => mq.removeEventListener('change', () => {});
  }, []);

  const handleUtilizationUpdate = useCallback(
    (utilization: Record<string, number>) => {
      setProviderUtilization(utilization);
    },
    []
  );

  return (
    <TooltipProvider>
      <SidebarProvider defaultOpen={true}>
        <AppSidebar
          page={page}
          setPage={setPage}
          providerUtilization={providerUtilization}
        />
        <SidebarInset>
          {page.view === 'dashboard' && (
            <Dashboard
              setPage={setPage}
              onUtilizationUpdate={handleUtilizationUpdate}
            />
          )}
          {page.view === 'provider' && (
            <ProviderDetail id={page.id} setPage={setPage} />
          )}
        </SidebarInset>
      </SidebarProvider>
    </TooltipProvider>
  );
}

export default App;
