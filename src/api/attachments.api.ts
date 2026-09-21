import { apiClient } from './client';
import type { Attachment } from '../types';

export const attachmentsApi = {
  async upload(conversationId: string, file: File, onProgress: (percent: number) => void) {
    const form = new FormData();
    form.append('file', file, file.name);
    return (
      await apiClient.post<Attachment>(`/conversations/${conversationId}/attachments`, form, {
        headers: { 'Content-Type': 'multipart/form-data' },
        timeout: 60000,
        onUploadProgress: (event) => {
          if (event.total)
            onProgress(Math.min(100, Math.round((event.loaded / event.total) * 100)));
        },
      })
    ).data;
  },
  async remove(id: string) {
    await apiClient.delete(`/attachments/${id}`);
  },
  async download(attachment: Attachment) {
    const response = await apiClient.get<Blob>(`/attachments/${attachment.id}/download`, {
      responseType: 'blob',
      timeout: 60000,
    });
    const url = URL.createObjectURL(response.data);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = attachment.originalName;
    anchor.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  },
};
