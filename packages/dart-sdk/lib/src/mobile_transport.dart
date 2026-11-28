import 'dart:async';
import 'package:http/http.dart' as http;
import 'scope.dart';
import 'managed.dart';
import 'pkce.dart';

/// Credentials issued by a reviewed human-mobile exchange. Memory only.
/// The native host must supply secure persistence and reviewed exchange/rotation.
final class MobileCredentials {
  final Scope scope;
  final String access, refresh;
  final Duration lifetime;
  final Stopwatch _age = Stopwatch()..start();
  MobileCredentials({required this.scope, required this.access,
    required this.refresh, required this.lifetime}) {
    if (!RegExp(r'^ovg_ma_[A-Za-z0-9_-]{43}$').hasMatch(access) ||
        !RegExp(r'^ovg_mr_[A-Za-z0-9_-]{43}$').hasMatch(refresh) ||
        lifetime <= Duration.zero || lifetime > const Duration(minutes:10)) {
      throw ArgumentError('Invalid mobile credentials');
    }
  }
  bool get expiring => _age.elapsed + const Duration(seconds:15) >= lifetime;
  @override String toString() => 'MobileCredentials(redacted)';
}

/// Trusted-host transport candidate. Never accepts web cookies or service keys.
/// A separate transport/client is required for each immutable project scope.
final class ScopedMobileTransport implements HumanTransport {
  @override final Scope scope;
  final http.Client _client;
  final Future<MobileCredentials> Function(String refresh) _rotate;
  final RefreshGate<MobileCredentials> _refresh = RefreshGate();
  MobileCredentials? _credentials;
  bool _closed = false;

  ScopedMobileTransport({required this.scope, required MobileCredentials credentials,
    required Future<MobileCredentials> Function(String refresh) rotate,
    required http.Client client}) : _credentials=credentials, _rotate=rotate, _client=client {
    if (!scope.same(credentials.scope)) throw ArgumentError('Credential scope mismatch');
  }

  bool _allowed(Uri uri) {
    if (uri.scheme != 'https' || uri.origin != scope.origin.origin ||
        uri.userInfo.isNotEmpty || uri.hasFragment) return false;
    final p = uri.pathSegments;
    if (p.length < 3 || p[0] != 'api' || p[1] != 'v1' ||
        p.any((s) => s.isEmpty || s == '.' || s == '..' || s.contains('/') || s.contains('\\'))) return false;
    if (p[2] == 'organizations') {
      if (p.length < 5 || p[3] != scope.organization) return false;
      if (p[4] == 'projects') return p.length >= 7 && p[5] == scope.project;
      return ['extensions','marketplace','chat'].contains(p[4]);
    }
    return (p.length == 4 && p[2] == 'marketplace' && p[3] == 'catalog') ||
        (p.length == 3 && p[2] == 'geocode') ||
        (p.length == 4 && p[2] == 'routing' && p[3] == 'route');
  }

  MobileCredentials _active() {
    final c = _credentials;
    if (_closed || c == null) throw StateError('SESSION_CLOSED');
    return c;
  }

  Future<MobileCredentials> _fresh() async {
    final current = _active();
    if (!current.expiring) return current;
    try {
      return await _refresh.run(() async {
        final old = _active();
        if (!old.expiring) return old;
        final next = await _rotate(old.refresh).timeout(const Duration(seconds:30));
        _active();
        if (!scope.same(next.scope) || next.refresh == old.refresh ||
            next.access == old.access || next.expiring) throw StateError('INVALID_ROTATION');
        _credentials = next;
        return next;
      });
    } catch (_) {
      close();
      throw StateError('REAUTHENTICATE');
    }
  }

  @override Future<http.StreamedResponse> send(http.Request request) async {
    _active();
    if (!_allowed(request.url) ||
        !['GET','POST','PATCH','PUT','DELETE'].contains(request.method) ||
        request.headers.keys.any((k) => !['accept','content-type'].contains(k.toLowerCase()))) {
      throw StateError('REQUEST_SCOPE_DENIED');
    }
    if (request.bodyBytes.length > 16*1024*1024) throw StateError('PAYLOAD_LIMIT');
    final credential = await _fresh();
    _active();
    final outgoing = http.Request(request.method, request.url)
      ..followRedirects=false
      ..maxRedirects=0
      ..headers.addAll(request.headers)
      ..headers['authorization']='Bearer ${credential.access}'
      ..bodyBytes=request.bodyBytes;
    // Never automatically retry or replay a request, including 401.
    final response = await _client.send(outgoing).timeout(const Duration(seconds:30));
    if (_closed || response.statusCode == 401 || response.statusCode == 403) {
      await response.stream.listen((_){}).cancel();
      close();
      throw StateError('REAUTHENTICATE');
    }
    if (response.statusCode >= 300 && response.statusCode < 400) {
      await response.stream.listen((_){}).cancel();
      throw const ProtocolError(403,'REDIRECT_DENIED');
    }
    return response;
  }

  @override void close() {
    if (_closed) return;
    _closed=true;
    _credentials=null;
    _refresh.revoke();
    _client.close();
  }
}
