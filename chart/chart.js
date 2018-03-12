function p2c(R, theta) {
    return {
        x: R * Math.cos(theta),
        y: R * Math.sin(theta)
    }
}

function addCenteredCircleRays(chart, params) {
    for (var angle = params.offset; angle<360+params.offset; angle += params.raySpacing) {
        console.log(angle);
        var angleRadians = angle * Math.PI / 180;
        console.log(angleRadians);
        var lineStart = p2c(params.radius, angleRadians);
        lineStart.x += chart.getWidth() / 2;
        lineStart.y += chart.getHeight() / 2;
        var lineEnd = p2c(params.radius + params.rayLength, angleRadians);
        lineEnd.x += chart.getWidth() / 2;
        lineEnd.y += chart.getHeight() / 2;
        chart.add(new fabric.Line([lineStart.x, lineStart.y, lineEnd.x, lineEnd.y], {
            fill: 'rgba(0,0,0,0)',
            stroke: params.stroke
        }));
    }
}

function addCenteredCircle(chart, radius) {
    chart.add(new fabric.Circle({
        left:chart.getWidth() / 2 - radius,
        top:chart.getHeight() / 2 - radius,
        radius: radius,
        fill: 'rgba(0,0,0,0)',
        stroke: 'black'
    }));
}

function addZodiac(chart) {
    addCenteredCircle(chart, 25);
    addCenteredCircle(chart, 300);
    addCenteredCircle(chart, 250);

    // 30 degree marks (sign boundaries)
    addCenteredCircleRays(chart, {
        radius: 250,
        fill: 'rgba(0,0,0,0)',
        stroke: 'red',
        rayLength: 50,
        raySpacing: 30,
        offset: 10
    });

    // 1 degree marks (degree boundaries)
    addCenteredCircleRays(chart, {
        radius: 250,
        fill: 'rgba(0,0,0,0)',
        stroke: 'green',
        rayLength: 7,
        raySpacing: 1,
        offset: 10
    });

}

var chart = new fabric.Canvas('chart');


addZodiac(chart);
