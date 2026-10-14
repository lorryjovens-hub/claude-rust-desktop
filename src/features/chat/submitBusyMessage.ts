import type { PendingFile } from '../../components/FileUploadPreview';
import { useBusySendStore } from '../../stores/useBusySendStore';

export async function submitBusyMessage(
  conversationId: string,
  text: string,
  files: PendingFile[],
  behavior: 'queue' | 'steer',
  interrupt: () => Promise<void>,
) {
  if (behavior === 'steer') await interrupt();
  useBusySendStore.getState().enqueue(conversationId, text, files, behavior === 'steer');
}
