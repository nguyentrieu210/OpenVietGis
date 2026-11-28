/// AI platform v1. Local workspace data must never be implicitly uploaded.
/// The authenticated native transport remains unavailable until SPEC073 qualifies it.
class AiPlatform {
  static const version = 1;
  static const cloudUrl = 'https://openvietgis.cloud/';
  static const nativeAuthenticated = false;
  static const actions = ['providers','run','acp','status','cancel'];
  static String status(bool vi) => vi
    ? 'AI cloud cần đăng nhập trong trình duyệt. Không gửi dữ liệu local.'
    : 'Cloud AI requires browser sign-in. Local data is not sent.';
}
