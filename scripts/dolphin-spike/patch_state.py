"""Narrow 2606a patch: checked synchronous host operations, original state format."""
import subprocess


def apply(source, rev):
    for name in ('State.h', 'State.cpp'):
        relative = 'Source/Core/Core/' + name
        original = subprocess.check_output(['git', 'show', f'{rev}:{relative}'], cwd=source, text=True)
        text = original
        if name.endswith('.h'):
            text = text.replace('void SaveAs(Core::System& system, std::string filename);', '''// Emulia host: call with a CPUThreadGuard; completion includes disk writes.
bool SaveAsChecked(Core::System& system, const std::string& filename);
bool LoadAsChecked(Core::System& system, const std::string& filename);
void SaveAs(Core::System& system, std::string filename);''')
        else:
            start = text.index('static void CompressAndDumpState(')
            end = text.index('static void SaveAsFromCore(', start)
            block = text[start:end].replace('static void CompressAndDumpState(', 'static bool CompressAndDumpState(')
            block = block.replace('    return;', '    return false;')
            block = block.replace('  if (!f.IsGood())\n    Core::DisplayMessage("Failed to write state file", 2000);', '''  if (!f.IsGood())
  {
    f.Close();
    File::Delete(temp_filename);
    return false;
  }''')
            # Close successfully before touching any previous save.
            block = block.replace('  const std::string last_state_filename', '''  if (!f.Close())
  {
    File::Delete(temp_filename);
    return false;
  }
  const std::string last_state_filename''')
            block = block.replace('  if (!f.Close())\n    Core::DisplayMessage("Failed to close state file", 2000);\n', '')
            block = block.replace('    Core::DisplayMessage("Failed to rename state file", 2000);', '    File::Delete(temp_filename);\n    return false;')
            block = block.rstrip()[:-1] + '  return true;\n}\n\n'
            text = text[:start] + block + text[end:]
            text = text.replace('static void LoadAsFromCore(', 'static bool LoadAsFromCore(')
            text = text.replace('    s_on_after_load_callback();\n}', '    s_on_after_load_callback();\n  return loaded_successfully;\n}')
            marker = 'void SetOnAfterLoadCallback('
            checked = '''bool SaveAsChecked(Core::System& system, const std::string& filename)
{
  s_compress_and_dump_thread.WaitForCompletion();
  Common::UniqueBuffer<u8> buffer;
  const auto size = SaveToBuffer(system, buffer);
  if (!size) return false;
  buffer.assign(buffer.extract().first, size);
  CompressAndDumpStateArgs args{.buffer = std::move(buffer), .filename = filename,
                               .task_lock = GetStateSaveTaskLock()};
  return CompressAndDumpState(system, args);
}

bool LoadAsChecked(Core::System& system, const std::string& filename)
{
  return CheckIfStateLoadIsAllowed(system) && LoadAsFromCore(system, filename);
}

'''
            text = text.replace(marker, checked + marker)
        path = source / relative
        if path.read_text() not in (original, text):
            raise RuntimeError(f'{relative} has unrelated edits; use a fresh work directory')
        if path.read_text() != text:
            path.write_text(text)
