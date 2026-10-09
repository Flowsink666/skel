/**
 * Core domain types and API contract definitions
 */

export interface ApiResponse<T> {
  code: number;
  data: T;
  message?: string;
  timestamp: number;
}

export interface UserProfile {
  id: string;
  name: string;
  email: string;
  roles: ('admin' | 'editor' | 'viewer')[];
  settings: Record<string, unknown>;
}

export type QueryParams = {
  page?: number;
  pageSize?: number;
  sortBy?: string;
  order?: 'asc' | 'desc';
};
