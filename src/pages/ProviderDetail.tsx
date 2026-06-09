import { useUsageReport } from '@/hooks/useUsageReport';
import { ProviderUsageCard, CacheIndicator } from '@/components/providers/ProviderUsageCard';
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

      {error && (
        <div
          className={`rounded-lg border p-3 text-sm ${
            error.kind === 'ReauthRequired'
              ? 'border-amber-300 bg-amber-50 text-amber-800'
              : 'border-red-300 bg-red-50 text-red-800'
          }`}
        >
          {error.kind === 'ReauthRequired' ? (
            <>
              <strong>Re-authentication required</strong>
              <p className="mt-1">{error.message}</p>
            </>
          ) : (
            <>
              {error.kind}: {error.message}
            </>
          )}
        </div>
      )}

      {loading && !report && (
        <div className="flex items-center justify-center py-12">
          <p className="text-muted-foreground">Loading usage report\u2026</p>
        </div>
      )}

      {report && <ProviderUsageCard report={report} />}
    </div>
  );
};
