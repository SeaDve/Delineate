const graphLoadedHandler = window.webkit.messageHandlers.graphLoaded;
const graphErrorHandler = window.webkit.messageHandlers.graphError;

class GraphView {
    constructor() {
        this.dotSrc = "";
        this.engine = "dot";

        this.svg = null;

        this.rendering = false;
        this.pendingUpdate = false;

        this.div = d3.select("#graph");
        this.graphviz = this.div.graphviz()
            .onerror(this._handleError.bind(this))
            .transition(() => {
                return d3.transition().duration(500);
            });

        d3.select(window).on("resize", () => {
            if (this.svg) {
                this.svg.attr("width", window.innerWidth).attr("height", window.innerHeight);
            }
        });
    }

    _handleError(error) {
        this.rendering = false;

        if (this.pendingUpdate) {
            this.pendingUpdate = false;
            this.renderGraph();
        }

        graphErrorHandler.postMessage(error);
    }

    _handleRenderReady() {
        this.svg = this.div.selectWithoutDataPropagation("svg");
        this.rendering = false;

        if (this.pendingUpdate) {
            this.pendingUpdate = false;
            this.renderGraph();
        }

        graphLoadedHandler.postMessage(null);
    }

    renderGraph() {
        if (this.dotSrc.length === 0) {
            graphLoadedHandler.postMessage(null);
            return;
        }

        if (this.rendering) {
            this.pendingUpdate = true;
            return;
        }

        this.svg = null;
        this.rendering = true;

        this.graphviz
            .width(window.innerWidth)
            .height(window.innerHeight)
            .fit(true)
            .engine(this.engine)
            .dot(this.dotSrc)
            .render(this._handleRenderReady.bind(this));
    }

    resetZoom() {
        if (!this.svg) {
            return;
        }

        const [, , svgWidth, svgHeight] = this.svg.attr("viewBox").split(' ');
        const graph0 = this.svg.selectWithoutDataPropagation("g");
        const bbox = graph0.node().getBBox();

        let { x, y } = d3.zoomTransform(this.graphviz.zoomSelection().node());
        const xOffset = (svgWidth - bbox.width) / 2;
        const yOffset = (svgHeight - bbox.height) / 2;
        x = -bbox.x + xOffset;
        y = -bbox.y + yOffset;

        const transform = d3.zoomIdentity.translate(x, y);
        this.graphviz.zoomSelection().call(this.graphviz.zoomBehavior().transform, transform);
    }

    getSvgString() {
        if (!this.svg) {
            return null;
        }

        const svg_node = this.svg.node();

        if (!svg_node) {
            return null;
        }

        // FIXME restore original translate
        const serializer = new XMLSerializer();

        return serializer.serializeToString(svg_node);
    }
}

const graphView = new GraphView();

function render(dotSrc, engine) {
    graphView.dotSrc = dotSrc;
    graphView.engine = engine;
    graphView.renderGraph();
}

function resetZoom() {
    graphView.resetZoom();
}

function getSvg() {
    return graphView.getSvgString();
}
