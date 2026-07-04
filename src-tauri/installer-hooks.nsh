; Hooks do instalador NSIS do Ember. So o pos-desinstalacao: limpa o que a app deixa FORA da
; pasta de instalacao (config, logs, marcador de primeiro arranque, entrada de autostart e as
; chaves de API no Windows Credential Manager). Sem isto, uma desinstalacao deixava segredos e
; estado para tras. Tudo best-effort: cada passo falha em silencio se o alvo nao existir.

!macro NSIS_HOOK_POSTUNINSTALL
  ; Autostart: o plugin escreve um valor no Run de HKCU. Nome pode ser "Ember" ou "ember".
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Ember"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ember"

  ; Config + marcador .installed (app_config_dir / app_data_dir = Roaming\{identifier}).
  RMDir /r "$APPDATA\com.deleg8lab.ember"
  ; Logs (app_log_dir = Local\{identifier}\logs).
  RMDir /r "$LOCALAPPDATA\com.deleg8lab.ember"

  ; Chaves de API no Credential Manager. O keyring monta o target como "{service}.{entry}";
  ; tenta ambas as ordens possiveis, cada uma falha em silencio se nao existir.
  nsExec::Exec 'cmdkey /delete:Ember.gemini_api_key'
  nsExec::Exec 'cmdkey /delete:Ember.claude_api_key'
  nsExec::Exec 'cmdkey /delete:Ember.openai_api_key'
  nsExec::Exec 'cmdkey /delete:gemini_api_key.Ember'
  nsExec::Exec 'cmdkey /delete:claude_api_key.Ember'
  nsExec::Exec 'cmdkey /delete:openai_api_key.Ember'
!macroend
