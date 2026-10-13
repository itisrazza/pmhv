import type { ApiError } from "../api/schemas";

export class ApiRequestError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "ApiRequestError";
    this.status = status;
    this.code = code;
  }
}

async function toRequestError(response: Response): Promise<ApiRequestError> {
  const body = await response.text();
  try {
    const error = JSON.parse(body) as ApiError;
    return new ApiRequestError(response.status, error.code, error.message);
  } catch {
    return new ApiRequestError(
      response.status,
      "unexpected_response",
      `${response.status} ${response.statusText}: ${body}`,
    );
  }
}

export const apiClient = async <T>(
  url: string,
  options: RequestInit,
): Promise<T> => {
  const response = await fetch(url, {
    ...options,
    headers: { Accept: "application/json", ...options.headers },
  });
  if (!response.ok) {
    throw await toRequestError(response);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  return (await response.json()) as T;
};

// orval picks this up from the mutator file and uses it as every hook's error type.
export type ErrorType<_Body> = ApiRequestError;
