// Idk why this exists
const WEIRD_BOTTOM_PADDING = 4;

const graphLoadedHandler = window.webkit.messageHandlers.graphLoaded;
const graphErrorHandler = window.webkit.messageHandlers.graphError;

class GraphView {
    constructor() {
        this.dotSrc = "";
        this.engine = "dot";

        this.rendering = false;
        this.pendingUpdate = false;

        this.div = d3.select("#graph");
        this.graphviz = this.div.graphviz()
            .onerror(this.handleError.bind(this))
            .transition(() => {
                return d3.transition().duration(500);
            });

        d3.select(window).on("resize", () => {
            let svg = this.div.selectWithoutDataPropagation("svg");
            svg.attr("width", window.innerWidth).attr("height", window.innerHeight - WEIRD_BOTTOM_PADDING);
        });
    }

    getSvgString() {
        // FIXME disregard width and height
        let svg = this.div.selectWithoutDataPropagation("svg").node();
        const serializer = new XMLSerializer();
        return svg ? serializer.serializeToString(svg) : null;
    }

    handleError(error) {
        this.rendering = false;

        if (this.pendingUpdate) {
            this.pendingUpdate = false;
            this.renderGraph();
        }

        graphErrorHandler.postMessage(error);
    }

    handleRenderReady() {
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

        this.rendering = true;
        this.graphviz
            .width(window.innerWidth)
            .height(window.innerHeight - WEIRD_BOTTOM_PADDING)
            .fit(true)
            .engine(this.engine)
            .dot(this.dotSrc)
            .render(this.handleRenderReady.bind(this));
    }
}

const graphView = new GraphView();

function render(dotSrc, engine) {
    graphView.dotSrc = dotSrc;
    graphView.engine = engine;
    graphView.renderGraph();
}

function getSvg() {
    return graphView.getSvgString();
}
