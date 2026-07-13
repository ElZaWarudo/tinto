const AGENT_SHELL_RESOLVER: &str = r#"set +u
load_tinto_agent_environment() {
  for profile in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile" "$HOME/.bashrc"; do
    if [ -r "$profile" ]; then
      . "$profile" >/dev/null 2>&1 || true
    fi
  done

  nvm_dir="${NVM_DIR:-$HOME/.nvm}"
  if [ -s "$nvm_dir/nvm.sh" ]; then
    export NVM_DIR="$nvm_dir"
    . "$nvm_dir/nvm.sh" >/dev/null 2>&1 || true
  fi

  candidate_path="$HOME/.local/bin:$HOME/.cargo/bin:$HOME/.npm-global/bin:$HOME/.volta/bin:$HOME/.bun/bin:$HOME/.local/share/pnpm:$HOME/.asdf/shims:$HOME/.local/share/mise/shims:$HOME/.local/share/fnm/aliases/default/bin:$HOME/.fnm/aliases/default/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"
  native_path=
  old_ifs=$IFS
  IFS=':'
  for path_entry in $candidate_path; do
    case "$path_entry" in
      /mnt/[A-Za-z]/*) continue ;;
    esac
    if [ -n "$path_entry" ]; then
      native_path="${native_path:+$native_path:}$path_entry"
    fi
  done
  IFS=$old_ifs
  export PATH="$native_path"
  hash -r 2>/dev/null || true
}

resolve_tinto_agent_binary() {
  agent_name=$1
  old_ifs=$IFS
  IFS=':'
  for path_entry in $PATH; do
    [ -n "$path_entry" ] || continue
    candidate="$path_entry/$agent_name"
    if [ ! -f "$candidate" ] || [ ! -x "$candidate" ]; then
      continue
    fi
    canonical=$(readlink -f -- "$candidate" 2>/dev/null || true)
    case "$canonical" in
      ""|/mnt/[A-Za-z]/*) continue ;;
    esac
    IFS=$old_ifs
    printf '%s\n' "$canonical"
    return 0
  done
  IFS=$old_ifs
  return 1
}
"#;

pub(crate) fn agent_console_script() -> String {
    format!(
        r#"{AGENT_SHELL_RESOLVER}
repo_path=${{1-}}
agent_name=${{2-}}
if [ -z "$repo_path" ] || [ -z "$agent_name" ]; then
  printf 'Tinto: faltan el repo o el agente para iniciar la sesion WSL.\n' >&2
  exit 127
fi
shift 2
if ! cd -- "$repo_path"; then
  printf 'Tinto: no se pudo abrir el repo WSL: %s\n' "$repo_path" >&2
  exit 127
fi

export TINTO_IADE=1
export TINTO_IADE_NAME='Integrated Agentic Development Environment'
export TINTO_TURN_DONE_MARKER='::tinto-turn-done::'
load_tinto_agent_environment

if ! resolved_agent=$(resolve_tinto_agent_binary "$agent_name"); then
  printf 'Tinto: no se encontro un ejecutable Linux de %s dentro de WSL. Instala el agente en esta distro.\n' "$agent_name" >&2
  exit 127
fi

agent_bin_dir=${{resolved_agent%/*}}
export PATH="$agent_bin_dir:$PATH"
if ! "$resolved_agent" --version >/dev/null 2>&1; then
  printf 'Tinto: el ejecutable WSL de %s no pudo iniciarse: %s\n' "$agent_name" "$resolved_agent" >&2
  exit 126
fi

exec "$resolved_agent" "$@"
status=$?
printf 'Tinto: no se pudo iniciar %s dentro de WSL (codigo %s).\n' "$agent_name" "$status" >&2
exit "$status"
"#
    )
}

pub(crate) fn agent_binary_check_script() -> String {
    format!(
        r#"{AGENT_SHELL_RESOLVER}
agent_name=${{1-}}
[ -n "$agent_name" ] || exit 1
load_tinto_agent_environment
if resolved_agent=$(resolve_tinto_agent_binary "$agent_name"); then
  agent_bin_dir=${{resolved_agent%/*}}
  export PATH="$agent_bin_dir:$PATH"
  if "$resolved_agent" --version >/dev/null 2>&1; then
    printf '%s\n' "$resolved_agent"
    exit 0
  fi
fi
exit 1
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_load_nvm_and_reject_windows_mount_targets() {
        for script in [agent_console_script(), agent_binary_check_script()] {
            assert!(script.contains("$nvm_dir/nvm.sh"));
            assert!(script.contains("readlink -f"));
            assert!(script.contains("/mnt/[A-Za-z]/*) continue"));
            assert!(script.contains("$resolved_agent\" --version"));
        }
    }

    #[test]
    fn console_script_reports_resolution_and_exec_failures() {
        let script = agent_console_script();

        assert!(script.contains("no se encontro un ejecutable Linux"));
        assert!(script.contains("no pudo iniciarse"));
        assert!(script.contains("no se pudo iniciar %s dentro de WSL"));
        assert!(!script.contains("find \"$root\""));
    }
}
