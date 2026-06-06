import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:nh_flutter_ui/src/rust/api/gallery.dart';

const _kApiKey = 'api_key';

/// Settings state
class SettingsState {
  final String apiKey;
  final bool isLoading;

  const SettingsState({
    required this.apiKey,
    this.isLoading = false,
  });

  SettingsState copyWith({String? apiKey, bool? isLoading}) {
    return SettingsState(
      apiKey: apiKey ?? this.apiKey,
      isLoading: isLoading ?? this.isLoading,
    );
  }
}

/// Settings notifier that persists API key to SharedPreferences
class SettingsNotifier extends Notifier<SettingsState> {
  @override
  SettingsState build() {
    return const SettingsState(apiKey: '');
  }

  /// Load settings from SharedPreferences. Call once at app startup.
  Future<void> load() async {
    final prefs = await SharedPreferences.getInstance();
    final savedKey = prefs.getString(_kApiKey) ?? '';
    state = state.copyWith(apiKey: savedKey);
  }

  /// Update the API key: persist to SharedPreferences and update Rust client.
  Future<void> updateApiKey(String newKey) async {
    state = state.copyWith(isLoading: true);
    try {
      // Persist locally
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(_kApiKey, newKey);

      // Update Rust-side client
      await nhSetApiKey(apiKey: newKey);

      state = state.copyWith(apiKey: newKey, isLoading: false);
    } catch (e) {
      state = state.copyWith(isLoading: false);
      rethrow;
    }
  }

  /// Reset key (clears stored key)
  Future<void> resetToDefault() async {
    await updateApiKey('');
  }
}

final settingsProvider = NotifierProvider<SettingsNotifier, SettingsState>(
  SettingsNotifier.new,
);