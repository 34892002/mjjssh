export * from './scripts'

export type RemoteTextEncoding = 'utf-8' | 'gbk' | 'gb18030'
export type RemoteTextLineEnding = 'lf' | 'crlf'
export type RemoteEditorLanguage = 'plain' | 'shell' | 'json' | 'yaml' | 'toml' | 'ini' | 'xml' | 'dockerfile' | 'sql' | 'terraform' | 'python' | 'go' | 'javascript' | 'typescript' | 'java' | 'kotlin' | 'php' | 'ruby' | 'perl' | 'lua' | 'markdown'

export interface RemoteFileMetadata {
  sessionId: string
  path: string
  size: number
  modifiedAt: string | null
  isSymlink: boolean
  isSupportedFile: boolean
}

export interface RemoteFileVersion {
  size: number
  modifiedAt: string | null
  contentHash: string
}

export interface RemoteTextFileBytes {
  bytes: number[]
  containsNul: boolean
  version: RemoteFileVersion
}

export interface OpenRemoteTextFileRequest {
  sessionId: string
  path: string
  allowLargeFile: boolean
}

export interface ExternalEditSession {
  editId: string
  sessionId: string
  path: string
  tempFileName: string
  localTempPath: string
  status: 'clean' | 'pending-upload' | 'uploading' | 'conflict' | 'error'
  version: RemoteFileVersion
}

export type ExternalEditSessionStatus = ExternalEditSession

export type UploadExternalEditResult =
  | { kind: 'uploaded'; version: RemoteFileVersion }
  | { kind: 'conflict'; currentVersion: RemoteFileVersion }

export interface SaveRemoteTextFileRequest {
  sessionId: string
  path: string
  content: string
  encoding: RemoteTextEncoding
  lineEnding: RemoteTextLineEnding
  expectedVersion: RemoteFileVersion
  force: boolean
  confirmBinaryWrite: boolean
}

export type AuthType = 'password' | 'key' | 'certificate'

export interface SshProfileView {
  id: string
  name: string
  host: string
  port: number
  username: string
  auth_type: AuthType
  key_id?: string
  proxy_id?: string
  group_name: string | null
  icon: string | null
  color: string | null
  os: string | null
  location: string | null
  created_at: string
  updated_at: string
}

export interface CreateProfileRequest {
  name: string
  host: string
  port?: number
  username: string
  auth_type: AuthType
  credential?: string
  key_id?: string
  proxy_id?: string
  clear_proxy?: boolean
  group_name?: string
  icon?: string
  color?: string
}

export interface UpdateProfileRequest {
  name?: string
  host?: string
  port?: number
  username?: string
  auth_type?: AuthType
  credential?: string
  key_id?: string
  proxy_id?: string
  clear_proxy?: boolean
  private_key?: string
  cert_data?: string
  group_name?: string
  icon?: string
  color?: string
}

export type Socks5ProxyAuthType = 'none' | 'password'

export interface Socks5ProxyView {
  id: string
  name: string
  host: string
  port: number
  auth_type: Socks5ProxyAuthType
  username?: string
  created_at: string
  updated_at: string
}

export interface CreateSocks5ProxyRequest {
  name: string
  host: string
  port: number
  auth_type: Socks5ProxyAuthType
  username?: string
  password?: string
}

export interface UpdateSocks5ProxyRequest extends CreateSocks5ProxyRequest {}

export interface SessionInfo {
  id: string
  profile_id: string
}

export interface TerminalSettings {
  terminalType: 'xterm-256color' | 'xterm' | 'vt100'
  fontSize: number
  fontFamily: string
  scrollbackLines: number
  backspaceSends: 'del' | 'bs'
  altSendsEscape: boolean
  connectTimeoutSeconds: number
  keepaliveIntervalSeconds: number
}

export interface SshKeyView {
  id: string
  name: string
  key_type: string
  algorithm: string
  created_at: string
  updated_at: string
}

export type ImportedSshKeyAlgorithm = 'auto' | 'ssh-rsa' | 'ssh-ed25519' | 'ssh-dss'

export interface CreateKeyRequest {
  name: string
  key_type: string
  algorithm?: ImportedSshKeyAlgorithm
  private_key: string
  cert_data?: string
}

export type SshKeyAlgorithm = 'ed25519' | 'rsa'

export interface GenerateSshKeyRequest {
  name: string
  algorithm: SshKeyAlgorithm
}

export interface GenerateSshKeyResult {
  key: SshKeyView
  publicKey: string
}
