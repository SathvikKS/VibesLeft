export interface UsageWindow {
  utilization: number;
  resets_at: string;
}

export interface UsageReport {
  provider_name: string;
  five_hour: UsageWindow;
  seven_day: UsageWindow;
  fetched_at_ms: number;
  cached: boolean;
}
