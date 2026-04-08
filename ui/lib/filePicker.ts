'use client'

import { directoryOpen, fileOpen } from 'browser-fs-access'

const IMAGE_EXTENSIONS = ['.png', '.jpg', '.jpeg', '.webp']
const ARCHIVE_EXTENSIONS = ['.cbz', '.zip', '.cb7', '.7z']
const SUPPORTED_EXTENSIONS = [...IMAGE_EXTENSIONS, ...ARCHIVE_EXTENSIONS]

export const pickImageFiles = async (): Promise<File[] | null> => {
  try {
    const files = await fileOpen({
      mimeTypes: [
        'image/*',
        'application/zip',
        'application/x-cbz',
        'application/x-7z-compressed',
      ],
      extensions: SUPPORTED_EXTENSIONS,
      multiple: true,
      description: 'Select images or comic book archives',
    })
    const result = Array.isArray(files) ? files : [files]
    return result.length > 0 ? result : null
  } catch {
    return null // user cancelled
  }
}

export const pickImageFolderFiles = async (): Promise<File[] | null> => {
  try {
    const files = await directoryOpen({ recursive: true })
    const supported = files.filter((f) =>
      SUPPORTED_EXTENSIONS.some((ext) => f.name.toLowerCase().endsWith(ext)),
    )
    return supported.length > 0 ? supported : null
  } catch {
    return null // user cancelled
  }
}
