import { useUsageReport } from '@/hooks/useUsageReport';
import { ProviderUsageCard, ProviderUsageCardSkeleton, CacheIndicator } from '@/components/providers/ProviderUsageCard';
import { UsageError } from '@/components/providers/UsageError';
import { Button } from '@/components/ui/button';
import { RefreshCw, ArrowLeft } from 'lucide-react';
import type { Page } from '@/App';

interface ProviderDetailProps {
  id: string;
  setPage: (page: Page) => void;
}

export const ProviderDetail = ({ id, setPage }: ProviderDetailProps) => {
  const { report, loading, error, refetch } = useUsageReport(id);

  return (
    <div className="flex flex-col gap-6 p-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Button
            variant="ghost"
            size="icon"
            onClick={() => setPage({ view: 'dashboard' })}
          >
            <ArrowLeft className="size-4" />
          </Button>
          <div>
            <h1 className="text-2xl font-bold text-foreground capitalize flex items-center gap-2">
              {id}
              {report && (
                <CacheIndicator
                  fetchedAtMs={report.fetched_at_ms}
                  cached={report.cached}
                />
              )}
            </h1>
            <p className="text-sm text-muted-foreground mt-1">
              API Consumption Metrics
            </p>
          </div>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={refetch}
          disabled={loading}
        >
          <RefreshCw
            className={`size-4 mr-1.5 ${loading ? 'animate-spin' : ''}`}
          />
          Refresh
        </Button>
      </div>

      {error && <UsageError error={error} />}

      {loading ? (
        <ProviderUsageCardSkeleton />
      ) : report ? (
        <ProviderUsageCard report={report} />
      ) : null}
    </div>
  );
};
