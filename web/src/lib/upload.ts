import { ApiRequestError } from "./api-client";

/**
 * Uploads a file as an image with progress reporting. fetch() can't report upload progress,
 * so this uses XMLHttpRequest.
 */
export function uploadImage(
  group: string,
  file: File,
  onProgress: (fraction: number) => void,
): Promise<void> {
  const url = `/api/v1/images/${encodeURIComponent(group)}/${encodeURIComponent(file.name)}`;
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open("PUT", url);
    request.setRequestHeader("Content-Type", "application/octet-stream");
    request.upload.addEventListener("progress", (event) => {
      if (event.lengthComputable) {
        onProgress(event.loaded / event.total);
      }
    });
    request.addEventListener("load", () => {
      if (request.status >= 200 && request.status < 300) {
        resolve();
        return;
      }
      try {
        const error = JSON.parse(request.responseText) as {
          code: string;
          message: string;
        };
        reject(new ApiRequestError(request.status, error.code, error.message));
      } catch {
        reject(
          new ApiRequestError(
            request.status,
            "unexpected_response",
            `${request.status}: ${request.responseText}`,
          ),
        );
      }
    });
    request.addEventListener("error", () =>
      reject(new Error("the upload was interrupted")),
    );
    request.send(file);
  });
}
