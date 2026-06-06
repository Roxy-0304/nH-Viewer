import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nh_flutter_ui/src/rust/api/gallery.dart';

/// 画廊详情状态管理
///
/// 使用 AsyncNotifier 管理单个画廊的详情数据，
/// 支持缓存和错误处理。
class GalleryDetailNotifier extends FamilyAsyncNotifier<GalleryInfo, BigInt> {
  @override
  Future<GalleryInfo> build(BigInt galleryId) async {
    return await nhGetGallery(id: galleryId);
  }

  /// 刷新画廊详情
  Future<void> refresh() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(() => build(arg));
  }
}

/// 画廊详情 Provider，按 galleryId 缓存
final galleryDetailProvider =
    AsyncNotifierProvider.family<GalleryDetailNotifier, GalleryInfo, BigInt>(
  GalleryDetailNotifier.new,
);

/// 搜索结果状态
class SearchState {
  final List<GalleryPreviewInfo> results;
  final bool isLoading;
  final String? error;
  final String query;
  final int currentPage;

  const SearchState({
    this.results = const [],
    this.isLoading = false,
    this.error,
    this.query = '',
    this.currentPage = 1,
  });

  SearchState copyWith({
    List<GalleryPreviewInfo>? results,
    bool? isLoading,
    String? error,
    String? query,
    int? currentPage,
  }) {
    return SearchState(
      results: results ?? this.results,
      isLoading: isLoading ?? this.isLoading,
      error: error,
      query: query ?? this.query,
      currentPage: currentPage ?? this.currentPage,
    );
  }
}

/// 搜索状态管理
///
/// 管理搜索查询、结果列表和分页状态。
class SearchNotifier extends Notifier<SearchState> {
  @override
  SearchState build() => const SearchState();

  /// 执行搜索
  Future<void> search(String query, {int page = 1}) async {
    if (query.trim().isEmpty) return;

    state = state.copyWith(
      isLoading: true,
      error: null,
      query: query,
      currentPage: page,
    );

    try {
      final results = await nhSearchGalleries(query: query, page: page);
      state = state.copyWith(
        results: page == 1 ? results : [...state.results, ...results],
        isLoading: false,
      );
    } catch (e) {
      state = state.copyWith(
        isLoading: false,
        error: e.toString(),
      );
    }
  }

  /// 加载下一页
  Future<void> loadNextPage() async {
    if (state.isLoading || state.query.isEmpty) return;
    await search(state.query, page: state.currentPage + 1);
  }

  /// 清除搜索结果
  void clear() {
    state = const SearchState();
  }
}

/// 搜索 Provider
final searchProvider = NotifierProvider<SearchNotifier, SearchState>(
  SearchNotifier.new,
);