import { useMemo, useEffect, useRef } from 'react';
import { useUsageReport } from '@/hooks/useUsageReport';
import { ENABLED_PROVIDERS } from '@/providers.config';
import { getStatusInfo, CacheIndicator } from '@/components/providers/ProviderUsageCard';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { Badge } from '@/components/ui/badge';
import { AlertTriangle, CheckCircle2 } from 'lucide-react';
import type { Page } from '@/App';

interface DashboardProps {
  setPage: (page: Page) => void;
  onUtilizationUpdate?: (utilization: Record<string, number>) => void;
}

const useProviderReport = (id: string) => {
  const { report, loading, error } = useUsageReport(id);
  return { report, loading, error, id };
};

const ArcGaugeDecoration = () => (
  <svg width="56" height="36" viewBox="0 0 56 36">
    <path d="M 4 34 A 24 24 0 0 1 52 34" stroke="#064e3b" strokeWidth="5" fill="none" strokeLinecap="round" />
    <path d="M 4 34 A 24 24 0 0 1 52 34" stroke="#34d399" strokeWidth="3" fill="none" strokeLinecap="round" />
  </svg>
);

const CriticalBarDecoration = () => (
  <div className="flex flex-col items-end gap-1.5">
    <div className="h-1 w-14 rounded-full bg-red-800/50" />
    <div className="h-1 w-8 rounded-full bg-red-500" />
    <div className="h-1 w-11 rounded-full bg-red-800/50" />
  </div>
);

const Sparkline = () => (
  <svg width="180" height="32" viewBox="0 0 180 32" className="overflow-visible">
    <polyline
      points="0,22 12,18 20,26 28,10 36,20 46,13 54,21 64,7 72,17 82,11 90,19 100,5 110,15 120,9 130,17 140,7 150,13 162,9 172,15 180,11"
      fill="none"
      stroke="#34d399"
      strokeWidth="1.5"
      strokeLinejoin="round"
      strokeLinecap="round"
    />
  </svg>
);

const AxisLabels = () => (
  <div className="flex justify-between text-[10px] text-muted-foreground mt-0.5 pl-0">
    {[0, 20, 40, 60, 80, 100].map((v) => (
      <span key={v}>{v}</span>
    ))}
  </div>
);

export const Dashboard = ({ setPage, onUtilizationUpdate }: DashboardProps) => {
  const results = ENABLED_PROVIDERS.map((p) => useProviderReport(p.id));

  const utilizations = useMemo(() => {
    const map: Record<string, number> = {};
    for (const r of results) {
      if (r.report) {
        map[r.id] = Math.max(r.report.five_hour.utilization, r.report.seven_day.utilization);
      }
    }
    return map;
  }, [results]);

  const prevUtilRef = useRef('');
  useEffect(() => {
    const serialized = JSON.stringify(utilizations);
    if (serialized !== prevUtilRef.current) {
      prevUtilRef.current = serialized;
      onUtilizationUpdate?.(utilizations);
    }
  }, [utilizations, onUtilizationUpdate]);

  const allReports = results.filter(
    (r): r is typeof r & { report: NonNullable<typeof r.report> } => r.report !== null,
  );

  const globalMaxUtilization =
    allReports.length > 0
      ? Math.max(...allReports.map((r) => Math.max(r.report.five_hour.utilization, r.report.seven_day.utilization)))
      : 0;

  const healthCounts = useMemo(() => {
    let healthy = 0;
    let warning = 0;
    let critical = 0;
    for (const r of allReports) {
      const maxU = Math.max(r.report.five_hour.utilization, r.report.seven_day.utilization);
      if (maxU >= 90) critical++;
      else if (maxU >= 75) warning++;
      else healthy++;
    }
    return { healthy, warning, critical };
  }, [allReports]);

  const overallStatus = healthCounts.critical > 0 ? 'critical' : 'healthy';
  const loadingCount = results.filter((r) => r.loading).length;
  const anyLoading = loadingCount > 0;

  return (
    <div className="flex flex-col gap-6 p-6">
      <div>
        <h1 className="text-2xl font-bold text-foreground">Dashboard</h1>
        <p className="text-sm text-muted-foreground mt-1">Aggregate usage across all providers</p>
      </div>

      {allReports.length > 0 && (
        <div
          className={`flex items-center gap-3 rounded-xl border p-4 ${
            overallStatus === 'critical'
              ? 'border-red-800/40 bg-red-950/30 dark:border-red-800/40 dark:bg-red-950/30'
              : 'border-emerald-800/40 bg-emerald-950/30 dark:border-emerald-800/40 dark:bg-emerald-950/30'
          }`}
        >
          {overallStatus === 'critical' ? (
            <AlertTriangle className="text-red-400 shrink-0" size={20} />
          ) : (
            <CheckCircle2 className="text-emerald-400 shrink-0" size={20} />
          )}
          <p
            className={`font-medium text-sm ${
              overallStatus === 'critical' ? 'text-red-300' : 'text-emerald-300'
            }`}
          >
            {overallStatus === 'critical'
              ? 'Vibes are off — one or more providers at critical usage'
              : 'Vibes are good — all providers within limits'}
          </p>
        </div>
      )}

      <div className="grid grid-cols-3 gap-4">
        {/* Healthy */}
        <div className="relative overflow-hidden rounded-xl border border-emerald-800/30 bg-emerald-950/30 p-4">
          <p className="text-sm text-emerald-400/70">Healthy</p>
          <p className="mt-1 text-3xl font-bold text-emerald-400">{anyLoading ? '—' : healthCounts.healthy}</p>
          <div className="absolute bottom-3 right-3">
            <ArcGaugeDecoration />
          </div>
        </div>

        {/* Warning */}
        <div className="relative overflow-hidden rounded-xl border border-amber-800/30 bg-amber-950/30 p-4">
          <p className="text-sm text-amber-400/70">Warning</p>
          <p className="mt-1 text-3xl font-bold text-amber-400">{anyLoading ? '—' : healthCounts.warning}</p>
          <div className="absolute bottom-3 right-3 opacity-50">
            <AlertTriangle size={36} className="text-amber-500" />
          </div>
        </div>

        {/* Critical */}
        <div className="relative overflow-hidden rounded-xl border border-red-800/30 bg-red-950/30 p-4">
          <p className="text-sm text-red-400/70">Critical</p>
          <p className="mt-1 text-3xl font-bold text-red-400">{anyLoading ? '—' : healthCounts.critical}</p>
          <div className="absolute bottom-3 right-3">
            <CriticalBarDecoration />
          </div>
        </div>

        {/* Most constrained window */}
        <div className="col-span-3 flex items-center justify-between rounded-xl border bg-card p-4">
          <div>
            <p className="text-sm text-muted-foreground">Most constrained window</p>
            <p className="mt-1 text-3xl font-bold text-foreground">
              {anyLoading ? 'Loading…' : `${globalMaxUtilization}%`}
            </p>
          </div>
          {!anyLoading && allReports.length > 0 && (
            <div className="flex items-end pb-1 opacity-90">
              <Sparkline />
            </div>
          )}
        </div>
      </div>

      <div className="flex flex-col gap-4">
        <h2 className="text-lg font-semibold text-foreground">Providers</h2>

        {ENABLED_PROVIDERS.map((provider) => {
          const result = results.find((r) => r.id === provider.id);
          const report = result?.report;
          const utilization = utilizations[provider.id] ?? 0;
          const statusInfo = getStatusInfo(utilization);

          return (
            <div key={provider.id} className="rounded-xl border bg-card p-5 shadow-sm">
              <div className="flex items-center gap-3 mb-4">
                <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-background border border-border">
                  <provider.icon size={20} className="text-foreground" />
                </div>
                <div className="flex flex-col gap-1">
                  <p className="font-semibold text-foreground leading-none">{provider.name}</p>
                  {report && (
                    <CacheIndicator fetchedAtMs={report.fetched_at_ms} cached={report.cached} />
                  )}
                </div>
              </div>

              {report ? (
                <div className="flex flex-col gap-2">
                  <div className="flex items-center gap-4">
                    <span className="w-24 shrink-0 text-sm text-muted-foreground">5h Window</span>
                    <div className="flex-1">
                      <Progress
                        value={report.five_hour.utilization}
                        indicatorClassName={getStatusInfo(report.five_hour.utilization).bg}
                        className="h-2"
                      />
                    </div>
                    <span className="w-10 text-right text-sm font-medium text-foreground">
                      {report.five_hour.utilization}%
                    </span>
                  </div>
                  <div className="flex items-center gap-4">
                    <span className="w-24 shrink-0 text-sm text-muted-foreground">7d Window</span>
                    <div className="flex-1">
                      <Progress
                        value={report.seven_day.utilization}
                        indicatorClassName={getStatusInfo(report.seven_day.utilization).bg}
                        className="h-2"
                      />
                    </div>
                    <span className="w-10 text-right text-sm font-medium text-foreground">
                      {report.seven_day.utilization}%
                    </span>
                  </div>
                  <div className="flex items-center gap-4">
                    <span className="w-24 shrink-0" />
                    <div className="flex-1">
                      <AxisLabels />
                    </div>
                    <span className="w-10" />
                  </div>
                  <div className="flex items-center justify-between mt-1">
                    <Badge
                      variant="outline"
                      className={
                        utilization >= 90
                          ? 'border-red-800/50 bg-red-950/40 text-red-400'
                          : utilization >= 75
                            ? 'border-amber-800/50 bg-amber-950/40 text-amber-400'
                            : 'border-emerald-800/50 bg-emerald-950/40 text-emerald-400'
                      }
                    >
                      {statusInfo.label}
                    </Badge>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setPage({ view: 'provider', id: provider.id })}
                    >
                      View details →
                    </Button>
                  </div>
                </div>
              ) : result?.loading ? (
                <p className="text-sm text-muted-foreground">Loading…</p>
              ) : result?.error ? (
                <p className="text-sm text-red-400">
                  {result.error.kind}: {result.error.message}
                </p>
              ) : null}
            </div>
          );
        })}
      </div>
    </div>
  );
};
