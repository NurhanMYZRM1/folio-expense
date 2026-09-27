export function errorMessage(error: unknown): string {
  if (
    typeof error === 'object' &&
    error !== null &&
    'message' in error &&
    typeof error.message === 'string'
  )
    return error.message;
  if (typeof error === 'string') return error;
  return 'The operation could not be completed. Your saved receipts remain available.';
}
