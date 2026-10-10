import { useMutation } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState } from 'react';
import i18n from '@/i18n';
import { extractFileToPath } from '@/lib/api/files';
import { errorMessage } from '@/lib/errors';
import { subscribeToEvent } from '@/lib/events/subscribers';
import { saveDialog } from '@/lib/platform/dialog';
import type {
  FileEntryRow,
  FileExtractionProgress,
  FileExtractionResult,
} from '@/types/models';

function validateDestinationPath(value: string) {
  const path = value.trim();
  if (!path) {
    return i18n.t('fileBrowser.extraction.destinationRequired');
  }
  const windowsDrivePath = /^[A-Za-z]:[\\/]/.test(path);
  const windowsUncPath = /^\\\\[^\\/]+[\\/][^\\/]+/.test(path);
  const posixPath = path.startsWith('/');
  if (!windowsDrivePath && !windowsUncPath && !posixPath) {
    return i18n.t('fileBrowser.extraction.destinationAbsolute');
  }
  return undefined;
}

function createOperationId() {
  return globalThis.crypto.randomUUID();
}

export function useFileExtractionModel() {
  const operationIdRef = useRef<string>();
  const inFlightRef = useRef(false);
  const [file, setFile] = useState<FileEntryRow>();
  const [formOpen, setFormOpen] = useState(false);
  const [resultOpen, setResultOpen] = useState(false);
  const [destinationPath, setDestinationPathState] = useState('');
  const [validationError, setValidationError] = useState<string>();
  const [interactionError, setInteractionError] = useState<string>();
  const [progress, setProgress] = useState<FileExtractionProgress>();
  const [result, setResult] = useState<FileExtractionResult>();

  const {
    mutateAsync,
    reset: resetExtraction,
    isPending,
    error: extractionError,
  } = useMutation({
    mutationFn: (request: Parameters<typeof extractFileToPath>[0]) => extractFileToPath(request),
    onSuccess: (nextResult) => {
      setResult(nextResult);
      setFormOpen(false);
      setResultOpen(true);
    },
  });

  useEffect(() => {
    const unsubscribe = subscribeToEvent<FileExtractionProgress>(
      'file-extract-progress',
      (event) => {
        if (event.payload.operationId === operationIdRef.current) {
          setProgress(event.payload);
        }
      },
    );
    return () => {
      unsubscribe();
    };
  }, []);

  const openForFile = useCallback((nextFile: FileEntryRow) => {
    if (nextFile.entryType !== 'file' || inFlightRef.current) {
      return;
    }
    operationIdRef.current = undefined;
    resetExtraction();
    setFile(nextFile);
    setDestinationPathState('');
    setValidationError(undefined);
    setInteractionError(undefined);
    setProgress(undefined);
    setResult(undefined);
    setResultOpen(false);
    setFormOpen(true);
  }, [resetExtraction]);

  const setDestinationPath = useCallback((value: string) => {
    setDestinationPathState(value);
    setValidationError(undefined);
    setInteractionError(undefined);
  }, []);

  const browseDestination = useCallback(async () => {
    if (!file || inFlightRef.current) {
      return;
    }
    try {
      const selectedPath = await saveDialog({ defaultPath: file.name || file.id });
      if (selectedPath) {
        setDestinationPath(selectedPath);
      }
    } catch (error) {
      setInteractionError(errorMessage(error, i18n.t('fileBrowser.extraction.destinationPickerFailed')));
    }
  }, [file, setDestinationPath]);

  const submit = useCallback(async () => {
    if (!file || inFlightRef.current) {
      return;
    }
    const pathError = validateDestinationPath(destinationPath);
    if (pathError) {
      setValidationError(pathError);
      return;
    }

    const operationId = createOperationId();
    operationIdRef.current = operationId;
    inFlightRef.current = true;
    setProgress(undefined);
    setInteractionError(undefined);
    try {
      await mutateAsync({
        operationId,
        fileId: file.id,
        destinationPath: destinationPath.trim(),
        overwrite: false,
      });
    } catch {
      // The typed mutation error remains visible in the form.
    } finally {
      inFlightRef.current = false;
    }
  }, [destinationPath, file, mutateAsync]);

  const setFormOpenSafely = useCallback((open: boolean) => {
    if (!inFlightRef.current && !isPending) {
      setFormOpen(open);
    }
  }, [isPending]);

  return {
    file,
    formOpen,
    resultOpen,
    destinationPath,
    validationError,
    error: interactionError ?? (extractionError
      ? errorMessage(extractionError, i18n.t('fileBrowser.extraction.failed'))
      : undefined),
    progress,
    result,
    isExtracting: isPending,
    openForFile,
    setFormOpen: setFormOpenSafely,
    setResultOpen,
    setDestinationPath,
    browseDestination,
    submit,
  };
}

export type FileExtractionModel = ReturnType<typeof useFileExtractionModel>;
