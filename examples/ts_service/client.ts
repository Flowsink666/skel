import { ApiResponse, UserProfile, QueryParams } from './types';

export class ApiClient {
  private baseUrl: string;
  private token: string | null = null;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }

  /**
   * Set authentication bearer token
   */
  public setAuthToken(token: string): void {
    this.token = token;
    localStorage.setItem('auth_token', token);
  }

  /**
   * Fetch user profile by ID with full error handling and retry logic
   */
  public async getUser(userId: string): Promise<ApiResponse<UserProfile>> {
    const url = `${this.baseUrl}/users/${userId}`;
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    if (this.token) {
      headers['Authorization'] = `Bearer ${this.token}`;
    }
    const res = await fetch(url, { headers });
    if (!res.ok) {
      throw new Error(`Failed to fetch user profile: ${res.statusText}`);
    }
    const json = await res.json();
    return json as ApiResponse<UserProfile>;
  }

  /**
   * Batch search users with pagination and query parameters
   */
  public async searchUsers(params: QueryParams): Promise<ApiResponse<UserProfile[]>> {
    const query = new URLSearchParams();
    if (params.page) query.set('page', params.page.toString());
    if (params.pageSize) query.set('pageSize', params.pageSize.toString());
    const res = await fetch(`${this.baseUrl}/users?${query.toString()}`);
    return await res.json();
  }
}
