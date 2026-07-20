export async function readJsonFiles(files: FileList) {
  const jsonFiles = Array.from(files).filter((file) => file.name.endsWith('.json'))
  const values = await Promise.all(
    jsonFiles.map(async (file) => JSON.parse(await file.text()) as unknown),
  )
  return values.flatMap((value) => Array.isArray(value) ? value : [value])
}
