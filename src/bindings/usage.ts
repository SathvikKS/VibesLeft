export interface UsageWindow {
  utilization: number;
  resets_at: string;
}

export interface UsageMetadata {
  primary_label: string;
  secondary_label: string;
  secondary_five_hour: UsageWindow;
  secondary_seven_day: UsageWindow;
}

export interface UsageReport {
  provider_name: string;
  five_hour: UsageWindow;
  seven_day: UsageWindow;
  fetched_at_ms: number;
  cached: boolean;
  metadata?: UsageMetadata;
}
