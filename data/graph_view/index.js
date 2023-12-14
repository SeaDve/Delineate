// TODO
// - no idea why we don't recover from errors
// - fix exporting
// - make mouse wheel zoom smooth like loupe
// - only allow resetZoom when not on original zoom level
// - improve packaging

const ZOOM_TRANSITION_DURATION_MS = 200;
const TRANSITION_DURATION_MS = 400;

const initEndHandler = window.webkit.messageHandlers.initEnd;
const errorHandler = window.webkit.messageHandlers.error;
const isRenderingChangedHandler = window.webkit.messageHandlers.isRenderingChanged;
const isGraphLoadedChangedHandler = window.webkit.messageHandlers.isGraphLoadedChanged;
const zoomLevelChangedHandler = window.webkit.messageHandlers.zoomLevelChanged;

class GraphView {
    constructor() {
        this._dotSrc = "";
        this._engine = "dot";

        this._prevDotSrc = this._dotSrc;
        this._prevEngine = this._engine;

        this._setSvg(null)
        this._setRendering(false);

        this._pendingUpdate = false;

        this._div = d3.select("#graph");
        this._graphviz = this._div.graphviz()
            .onerror(this._handleError.bind(this))
            .on('initEnd', this._handleInitEnd.bind(this))
            .transition(() => {
                return d3.transition().duration(TRANSITION_DURATION_MS);
            });

        d3.select(window).on("resize", () => {
            if (this._svg) {
                this._svg.attr("width", window.innerWidth).attr("height", window.innerHeight);
            }
        });
    }

    _handleError(error) {
        this._setRendering(false)

        if (this._pendingUpdate) {
            this._pendingUpdate = false;
            this._renderGraph();
        }

        errorHandler.postMessage(error);
    }

    _handleInitEnd() {
        initEndHandler.postMessage(null);
        zoomLevelChangedHandler.postMessage(this._getZoomLevel());

        this._renderGraph();
    }

    _handleRenderDone() {
        this._setSvg(this._div.selectWithoutDataPropagation("svg"));
        this._setRendering(false);

        this._graphviz.zoomBehavior().on("end", this._handleZoomEnd.bind(this));

        if (this._pendingUpdate) {
            this._pendingUpdate = false;
            this._renderGraph();
        }

        zoomLevelChangedHandler.postMessage(this._getZoomLevel());
    }

    _handleZoomEnd() {
        zoomLevelChangedHandler.postMessage(this._getZoomLevel());
    }

    _setRendering(rendering) {
        this._rendering = rendering;
        isRenderingChangedHandler.postMessage(rendering);
    }

    _setSvg(svg) {
        this._svg = svg;
        isGraphLoadedChangedHandler.postMessage(svg !== null);
    }

    _getZoomLevel() {
        if (!this._svg) {
            return 1;
        }

        return d3.zoomTransform(this._svg.node()).k;
    }

    _renderGraph() {
        if (this._dotSrc.length === 0) {
            if (this._svg) {
                this._svg.remove();
                this._setSvg(null)
            }
            this._setRendering(false)
            return;
        }

        if (this._dotSrc === this._prevDotSrc && this._engine === this._prevEngine) {
            this._setRendering(false)
            return;
        }

        if (this._rendering) {
            this._pendingUpdate = true;
            return;
        }

        this._setSvg(null)
        this._setRendering(true)

        this._graphviz
            .width(window.innerWidth)
            .height(window.innerHeight)
            .fit(true)
            .engine(this._engine)
            .dot(this._dotSrc)
            .render(this._handleRenderDone.bind(this));
    }

    graphvizVersion() {
        return this._graphviz.graphvizVersion();
    }

    setData(dotSrc, engine) {
        this._prevDotSrc = this._dotSrc;
        this._prevEngine = this._engine;

        this._dotSrc = dotSrc;
        this._engine = engine;

        this._renderGraph();
    }

    setZoomScaleExtent(min, max) {
        this._graphviz.zoomScaleExtent([min, max]);
    }

    setZoomLevelBy(factor) {
        if (!this._svg) {
            return;
        }

        this._graphviz.zoomSelection()
            .transition(() => {
                return d3.transition().duration(ZOOM_TRANSITION_DURATION_MS);
            })
            .call(this._graphviz.zoomBehavior().scaleBy, factor);
    }

    resetZoom() {
        if (!this._svg) {
            return;
        }

        const transition = d3.transition().duration(ZOOM_TRANSITION_DURATION_MS);
        this._graphviz.resetZoom(transition);
    }

    getSvgString() {
        if (!this._svg) {
            return null;
        }

        const svg_node = this._svg.node();

        if (!svg_node) {
            return null;
        }

        // FIXME restore original translate
        const serializer = new XMLSerializer();

        return serializer.serializeToString(svg_node);
    }
}

const graphView = new GraphView();
