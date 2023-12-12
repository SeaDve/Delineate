const graphLoadedHandler = window.webkit.messageHandlers.graphLoaded;
const graphErrorHandler = window.webkit.messageHandlers.graphError;

class GraphView {
    constructor() {
        this._dotSrc = "";
        this._engine = "dot";

        this._svg = null;

        this._rendering = false;
        this._pendingUpdate = false;

        this._div = d3.select("#graph");
        this._graphviz = this._div.graphviz()
            .onerror(this._handleError.bind(this))
            .transition(() => {
                return d3.transition().duration(500);
            });

        d3.select(window).on("resize", () => {
            if (this._svg) {
                this._svg.attr("width", window.innerWidth).attr("height", window.innerHeight);
            }
        });
    }

    _handleError(error) {
        this._rendering = false;

        if (this._pendingUpdate) {
            this._pendingUpdate = false;
            this._renderGraph();
        }

        graphErrorHandler.postMessage(error);
    }

    _handleRenderReady() {
        this._svg = this._div.selectWithoutDataPropagation("svg");
        this._rendering = false;

        if (this._pendingUpdate) {
            this._pendingUpdate = false;
            this._renderGraph();
        }

        graphLoadedHandler.postMessage(null);
    }

    _renderGraph() {
        if (this._dotSrc.length === 0) {
            graphLoadedHandler.postMessage(null);
            return;
        }

        if (this._rendering) {
            this._pendingUpdate = true;
            return;
        }

        this._svg = null;
        this._rendering = true;

        this._graphviz
            .width(window.innerWidth)
            .height(window.innerHeight)
            .fit(true)
            .engine(this._engine)
            .dot(this._dotSrc)
            .render(this._handleRenderReady.bind(this));
    }

    setData(dotSrc, engine) {
        this._dotSrc = dotSrc;
        this._engine = engine;
        this._renderGraph();
    }

    resetZoom() {
        if (!this._svg) {
            return;
        }

        const [, , svgWidth, svgHeight] = this._svg.attr("viewBox").split(' ');
        const graph0 = this._svg.selectWithoutDataPropagation("g");
        const bbox = graph0.node().getBBox();

        let { x, y } = d3.zoomTransform(this._graphviz.zoomSelection().node());
        const xOffset = (svgWidth - bbox.width) / 2;
        const yOffset = (svgHeight - bbox.height) / 2;
        x = -bbox.x + xOffset;
        y = -bbox.y + yOffset;

        const transform = d3.zoomIdentity.translate(x, y);
        this._graphviz.zoomSelection().call(this._graphviz.zoomBehavior().transform, transform);
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
