import { processFile } from './imageProcessor'
import type { PreprocessingPrefs } from '../types/preprocessing'

interface WorkerMessage {
  file: File
  prefs: PreprocessingPrefs
}

interface WorkerResponse {
  success: boolean
  file?: File
  error?: string
}

self.onmessage = async (e: MessageEvent<WorkerMessage>) => {
  const { file, prefs } = e.data
  try {
    const processed = await processFile(file, prefs)
    const response: WorkerResponse = { success: true, file: processed }
    self.postMessage(response)
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Processing failed'
    const response: WorkerResponse = { success: false, error: message }
    self.postMessage(response)
  }
}
