import { useMutation } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState } from 'react';
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
    return '请选择或填写提取目标路径。';
  }
  const windowsDrivePath = /^[A-Za-z]:[\\/]/.test(path);
  const windowsUncPath = /^\\\\[^\\/]+[\\/][^\\/]+/.test(path);
  const posixPath = path.startsWith('/');
  if (!windowsDrivePath && !windowsUncPath && !posixPath) {
    return '目标路径必须是绝对路径。';
  }
  return undefined;
}

function createOperationId() {
  return globalThis.crypto.randomUUID();
}

export function useFileExtractionModel() {
  const operationIdRef = useRef<string>();
  const [file, setFile] = useState<FileEntryRow>();
  const [formOpen, setFormOpen] = useState(false);
  const [resultOpen, setResultOpen] = useState(false);
  const [destinationPath, setDestinationPathState] = useState('');
  const [validationError, setValidationError] = useState<string>();
  const [interactionError, setInteractionError] = useState<string>();
  const [progress, setProgress] = useState<FileExtractionProgress>();
  const [result, setResult] = useState<FileExtractionResult>();

  const extraction = useMutation({
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
    if (nextFile.entryType !== 'file') {
      return;
    }
    operationIdRef.current = undefined;
    extraction.reset();
    setFile(nextFile);
    setDestinationPathState('');
    setValidationError(undefined);
    setInteractionError(undefined);
    setProgress(undefined);
    setResult(undefined);
    setResultOpen(false);
    setFormOpen(true);
  }, [extraction]);

  const setDestinationPath = useCallback((value: string) => {
    setDestinationPathState(value);
    setValidationError(undefined);
    setInteractionError(undefined);
  }, []);

  const browseDestination = useCallback(async () => {
    if (!file || extraction.isPending) {
      return;
    }
    try {
      const selectedPath = await saveDialog({ defaultPath: file.name || file.id });
      if (selectedPath) {
        setDestinationPath(selectedPath);
      }
    } catch (error) {
      setInteractionError(errorMessage(error, '无法打开目标路径选择器。'));
    }
  }, [extraction.isPending, file, setDestinationPath]);

  const submit = useCallback(async () => {
    if (!file || extraction.isPending) {
      return;
    }
    const pathError = validateDestinationPath(destinationPath);
    if (pathError) {
      setValidationError(pathError);
      return;
    }

    const operationId = createOperationId();
    operationIdRef.current = operationId;
    setProgress(undefined);
    setInteractionError(undefined);
    try {
      await extraction.mutateAsync({
        operationId,
        fileId: file.id,
        destinationPath: destinationPath.trim(),
        overwrite: false,
      });
    } catch {
      // The typed mutation error remains visible in the form.
    }
  }, [destinationPath, extraction, file]);

  const setFormOpenSafely = useCallback((open: boolean) => {
    if (!extraction.isPending) {
      setFormOpen(open);
    }
  }, [extraction.isPending]);

  return {
    file,
    formOpen,
    resultOpen,
    destinationPath,
    validationError,
    error: interactionError ?? (extraction.error
      ? errorMessage(extraction.error, '文件提取失败。')
      : undefined),
    progress,
    result,
    isExtracting: extraction.isPending,
    openForFile,
    setFormOpen: setFormOpenSafely,
    setResultOpen,
    setDestinationPath,
    browseDestination,
    submit,
  };
}

export type FileExtractionModel = ReturnType<typeof useFileExtractionModel>;
