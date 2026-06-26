import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { UsageError } from '@/components/providers/UsageError';
import { getStatusInfo, CacheIndicator } from '@/components/providers/ProviderUsageCard';
import type { UsageReport } from '@/bindings/usage';
import type { ProviderConfig } from '@/providers.config';

interface ProviderSummaryCardProps {
  provider: ProviderConfig;
  report: UsageReport | null;
  loading: boolean;
  error: { kind: string; message: string } | null;
  utilization: number;
  onClick: () => void;
}

export const ProviderSummaryCard = ({
  provider,
  report,
  loading,
  error,
  utilization,
  onClick,
}: ProviderSummaryCardProps) => {
  const statusInfo = getStatusInfo(utilization);

  return (
    <div
      className={`rounded-xl border bg-card p-5 shadow-sm cursor-pointer transition-colors ${
        report
          ? utilization >= 90
            ? 'border-red-800/50 hover:border-red-700/60'
            : utilization >= 75
              ? 'border-amber-800/50 hover:border-amber-700/60'
              : 'border-emerald-800/50 hover:border-emerald-700/60'
          : 'hover:border-border/80'
      }`}
      onClick={onClick}
    >
      <div className="flex items-start gap-3 mb-5">
        <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-background border border-border shrink-0">
          <provider.icon size={20} className="text-foreground" />
        </div>
        <div className="flex flex-col gap-1 min-w-0 flex-1">
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-2 min-w-0">
              <p className="font-semibold text-foreground leading-tight">{provider.name}</p>
              {report && (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <div className={`w-2 h-2 rounded-full shrink-0 cursor-default ${statusInfo.bg}`} />
                  </TooltipTrigger>
                  <TooltipContent>{statusInfo.label}</TooltipContent>
                </Tooltip>
              )}
            </div>
          </div>
          {report && (
            <CacheIndicator fetchedAtMs={report.fetched_at_ms} cached={report.cached} />
          )}
          {loading && !report && <Skeleton className="h-5 w-20 rounded-md" />}
        </div>
      </div>

      {report ? (
        <div className="flex flex-col gap-3">
          <div>
            <div className="flex items-center justify-between mb-1.5">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                5H Window
              </span>
              <span className="text-sm font-medium text-foreground">
                {report.five_hour.utilization}%
              </span>
            </div>
            <Progress
              value={report.five_hour.utilization}
              indicatorClassName={getStatusInfo(report.five_hour.utilization).bg}
              className="h-1.5"
            />
          </div>
          <div>
            <div className="flex items-center justify-between mb-1.5">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                7D Window
              </span>
              <span className="text-sm font-medium text-foreground">
                {report.seven_day.utilization}%
              </span>
            </div>
            <Progress
              value={report.seven_day.utilization}
              indicatorClassName={getStatusInfo(report.seven_day.utilization).bg}
              className="h-1.5"
            />
          </div>
        </div>
      ) : loading ? (
        <div className="flex flex-col gap-3">
          {[0, 1].map((i) => (
            <div key={i}>
              <div className="flex items-center justify-between mb-1.5">
                <Skeleton className="h-2.5 w-16" />
                <Skeleton className="h-4 w-8" />
              </div>
              <Skeleton className="h-1.5 w-full rounded-full" />
            </div>
          ))}
        </div>
      ) : error ? (
        <UsageError error={error} compact />
      ) : null}
    </div>
  );
};
