// No drag and drop of files on a phone.
export function getCurrentWebviewWindow() {
  return {
    onDragDropEvent: async (_handler: (event: { payload: { type: string; paths: string[] } }) => void) => () => undefined,
  };
}
