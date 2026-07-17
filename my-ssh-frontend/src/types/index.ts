export type AuthType = 'password' | 'key' | 'certificate'

export interface SshProfileView {
  id: string
  name: string
  host: string
  port: number
  username: string
  auth_type: AuthType
  key_id?: string
  group_name: string | null
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
  group_name?: string
}

export interface UpdateProfileRequest {
  name?: string
  host?: string
  port?: number
  username?: string
  auth_type?: AuthType
  credential?: string
  private_key?: string
  cert_data?: string
  group_name?: string
}

export interface SessionInfo {
  id: string
  profile_id: string
}

export interface SshKeyView {
  id: string
  name: string
  key_type: string
  created_at: string
  updated_at: string
}

export interface CreateKeyRequest {
  name: string
  key_type: string
  private_key: string
  cert_data?: string
}
